//! Per-domain async port traits + mcp-core-local serde DTOs (phase 3.2).
//!
//! Ports-and-adapters seam: the `#[tool]` methods in [`crate::router`] are thin
//! adapters that call these traits and JSON-encode the returned DTOs. The REAL
//! implementations live in the Chunk-2 Tauri monolith (where redaction +
//! precision-reduction happen at the impl boundary); the Pi-buildable tier-2
//! [`tuxlink-mcp-testserver`](../../tuxlink-mcp-testserver) supplies canned mock
//! impls so the spine is exercised end-to-end without the app.
//!
//! **Redaction is NOT a port concern.** Ports return ALREADY-CURATED DTO shapes:
//! the impl is responsible for stripping secrets / reducing grid precision /
//! minimizing MACs before the DTO crosses this boundary. The agent-facing DTOs
//! here therefore carry no password/secret fields by construction.
//!
//! **Taint IS the router's concern, not the port's.** Methods marked `[TAINT]`
//! in the design return untrusted external content; the calling `#[tool]`
//! adapter calls [`EgressGuard::taint`](tuxlink_security::EgressGuard::taint)
//! AFTER a successful port return. Ports never touch the guard.
//!
//! All traits are `Send + Sync` and object-safe so [`crate::McpState`] can hold
//! them as `Arc<dyn Port>`.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Failure modes a port adapter can surface to the agent. The router maps these
/// onto rmcp tool errors; the impl chooses the variant.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum PortError {
    /// The requested entity (folder, message id, …) does not exist.
    #[error("not found")]
    NotFound,
    /// The capability is temporarily unavailable (backend offline, modem not
    /// connected, …). Carries an operator-facing reason.
    #[error("unavailable: {0}")]
    Unavailable(String),
    /// An internal error occurred fulfilling the request.
    #[error("internal error: {0}")]
    Internal(String),
}

// ---------------------------------------------------------------------------
// DTOs — mcp-core-local agent-facing shapes. Minimal by design; the monolith
// impl curates the real values into these. No secret/password fields.
// ---------------------------------------------------------------------------

/// One message's metadata in a folder listing or search result. No body — the
/// body is fetched via [`MailboxPort::read`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MessageMetaDto {
    pub id: String,
    pub subject: String,
    pub from: String,
    pub to: String,
    pub date: String,
    pub unread: bool,
    pub has_attachments: bool,
}

/// One attachment's curated metadata. No bytes — attachment payloads are out of
/// scope for the read tier.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AttachmentMetaDto {
    pub filename: String,
    pub size: u64,
}

/// A fully parsed message body + headers, returned by [`MailboxPort::read`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ParsedMessageDto {
    pub id: String,
    pub subject: String,
    pub from: String,
    pub to: String,
    pub cc: String,
    pub date: String,
    pub body: String,
    pub attachments: Vec<AttachmentMetaDto>,
    pub has_form: bool,
}

/// A mailbox folder + its message count.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FolderDto {
    pub name: String,
    pub count: u32,
}

/// Search input. `folder` scopes the search; `limit` caps the result count.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SearchQueryDto {
    pub query: String,
    pub folder: Option<String>,
    pub limit: Option<u32>,
}

/// Search output: the matched message metadata plus the total match count
/// (which may exceed `items.len()` when `limit` truncated the page).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SearchResultsDto {
    pub items: Vec<MessageMetaDto>,
    pub total: u32,
}

/// One in-app documentation search hit.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DocsHitDto {
    pub title: String,
    pub path: String,
    pub snippet: String,
}

/// One template-catalog entry (forms / standard messages).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CatalogEntryDto {
    pub id: String,
    pub title: String,
    pub category: String,
}

/// Curated, non-secret view of the top-level config. `grid` is already
/// precision-reduced to a 4-char Maidenhead locator by the impl.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConfigViewDto {
    pub connect_to_cms: bool,
    pub transport: String,
    pub host: String,
    pub callsign: String,
    /// Maidenhead locator, already reduced to 4 chars by the impl.
    pub grid: String,
}

/// Non-secret ARDOP modem config.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArdopConfigDto {
    pub host: String,
    pub port: u16,
    pub drive_level: u8,
    pub bandwidth: u32,
}

/// Non-secret VARA modem config. No VARA license/registration secrets.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VaraConfigDto {
    pub host: String,
    pub port: u16,
    pub bandwidth: u32,
    pub drive_level: u8,
}

/// Non-secret packet (AX.25 / KISS) config.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PacketConfigDto {
    pub kiss_host: String,
    pub kiss_port: u16,
    pub baud: u32,
    pub tx_delay: u32,
}

/// One serial device the operator can pick for a TNC / CAT connection.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SerialDeviceDto {
    pub path: String,
    pub description: String,
}

/// One Bluetooth device. `mac` is already minimized/partially-masked by the
/// impl; this tier never exposes a full address as a fingerprintable secret.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BluetoothDeviceDto {
    pub name: String,
    pub mac: String,
}

/// Capture + playback audio device names for modem audio selection.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AudioDevicesDto {
    pub capture: Vec<String>,
    pub playback: Vec<String>,
}

/// Live backend (CMS connection / engine) status.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BackendStatusDto {
    pub connected: bool,
    pub transport: String,
    pub state: String,
}

/// Live modem status.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModemStatusDto {
    pub kind: String,
    pub connected: bool,
    pub state: String,
}

/// Live VARA modem status.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VaraStatusDto {
    pub connected: bool,
    pub bandwidth: u32,
    pub state: String,
}

/// Current position status. `grid` is precision-reduced by the impl.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PositionStatusDto {
    pub has_fix: bool,
    pub grid: String,
    pub source: String,
}

/// Host platform info for diagnostics.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlatformInfoDto {
    pub os: String,
    pub arch: String,
    pub app_version: String,
}

/// One session-log line (already redacted at the impl's sink boundary).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LogLineDto {
    pub timestamp: String,
    pub level: String,
    pub message: String,
}

// ---------------------------------------------------------------------------
// Port traits.
// ---------------------------------------------------------------------------

/// Read-only status + diagnostic queries. None taint.
#[async_trait]
pub trait StatusPort: Send + Sync {
    async fn backend_status(&self) -> Result<BackendStatusDto, PortError>;
    async fn modem_status(&self) -> Result<ModemStatusDto, PortError>;
    async fn vara_status(&self) -> Result<VaraStatusDto, PortError>;
    async fn position_status(&self) -> Result<PositionStatusDto, PortError>;
    async fn platform_info(&self) -> Result<PlatformInfoDto, PortError>;
    async fn wizard_completed(&self) -> Result<bool, PortError>;
    /// Whether a stored P2P peer password is Set or NotSet for `callsign`.
    /// Returns the boolean only — never the password — so this is NOT a taint
    /// source.
    async fn p2p_peer_password_status(&self, callsign: &str) -> Result<bool, PortError>;
}

/// Mailbox reads. `list` + `read` return untrusted message content → the
/// calling tool taints; `folders` is structural metadata and does not.
#[async_trait]
pub trait MailboxPort: Send + Sync {
    /// List a folder's messages. **TAINT** (untrusted subjects/senders).
    async fn list(&self, folder: &str) -> Result<Vec<MessageMetaDto>, PortError>;
    /// Read one parsed message. **TAINT** (untrusted body/headers).
    async fn read(&self, folder: &str, id: &str) -> Result<ParsedMessageDto, PortError>;
    /// Enumerate folders + counts. Structural metadata; does not taint.
    async fn folders(&self) -> Result<Vec<FolderDto>, PortError>;
}

/// Search across mailbox, docs, and the template catalog. `messages` returns
/// untrusted content → the calling tool taints; `docs` + `catalog` are
/// app-owned content and do not.
#[async_trait]
pub trait SearchPort: Send + Sync {
    /// Search mailbox messages. **TAINT** (untrusted content).
    async fn messages(&self, query: SearchQueryDto) -> Result<SearchResultsDto, PortError>;
    /// Search in-app documentation. App-owned content; does not taint.
    async fn docs(&self, query: &str) -> Result<Vec<DocsHitDto>, PortError>;
    /// List the template catalog. App-owned content; does not taint.
    async fn catalog(&self) -> Result<Vec<CatalogEntryDto>, PortError>;
}

/// Curated, non-secret config reads. None taint (app-owned config).
#[async_trait]
pub trait ConfigPort: Send + Sync {
    async fn read(&self) -> Result<ConfigViewDto, PortError>;
    async fn ardop(&self) -> Result<ArdopConfigDto, PortError>;
    async fn vara(&self) -> Result<VaraConfigDto, PortError>;
    async fn packet(&self) -> Result<PacketConfigDto, PortError>;
}

/// Hardware device enumeration. None taint (device names, not message content).
#[async_trait]
pub trait DevicePort: Send + Sync {
    async fn serial(&self) -> Result<Vec<SerialDeviceDto>, PortError>;
    async fn bluetooth(&self) -> Result<Vec<BluetoothDeviceDto>, PortError>;
    async fn audio(&self) -> Result<AudioDevicesDto, PortError>;
}

/// Session-log snapshot. The snapshot can carry untrusted wire content → the
/// calling tool taints.
#[async_trait]
pub trait LogPort: Send + Sync {
    /// Snapshot the current session log. **TAINT** (may contain untrusted wire
    /// content even after sink redaction).
    async fn snapshot(&self) -> Result<Vec<LogLineDto>, PortError>;
}
