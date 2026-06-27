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

use std::collections::BTreeMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::validate::ValidationError;

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

// ---------------------------------------------------------------------------
// Egress (phase 3.3) — gated capability + ungated abort.
//
// EgressPort methods are already-gated Agent operations: every IMPL runs the
// real work through `tuxlink_security::guarded_egress(.., Agent, ..)` so the
// armed/taint/poison gate is enforced AT the impl, not at the router. The
// trait merely EXPOSES the capability; the router #[tool] is a thin adapter.
// AbortPort is the dual: stopping is ALWAYS allowed and never gated.
// ---------------------------------------------------------------------------

/// Failure modes an egress (transmit/connect) op can surface to the agent.
/// `Denied` carries the egress-gate refusal reason (unarmed / expired / tainted
/// / poisoned); `Failed` carries an operational failure AFTER the gate passed.
/// The router maps `Denied` onto an authorization-shaped tool error.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum EgressPortError {
    /// The egress gate refused the Agent caller. The string is the
    /// `EgressDenied` reason (e.g. "send authority is not armed").
    #[error("egress denied: {0}")]
    Denied(String),
    /// The egress was authorized but the operation itself failed.
    #[error("egress failed: {0}")]
    Failed(String),
}

/// Which message POOL / routing a B2F session targets. Mirrors the monolith's
/// `SessionIntent` 1:1 (`Cms` / `RadioOnly` / `PostOffice` / `Mesh` / `P2p`);
/// the impl maps it onto `crate::winlink::session::SessionIntent`.
/// A B2F exchange always performs a full send+receive round once connected, so
/// this selects the routing pool, not a transfer direction.
#[derive(
    Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, schemars::JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum SessionIntentDto {
    /// Global Winlink CMS (Telnet/TLS or transparent relay-to-CMS proxy).
    #[default]
    Cms,
    /// R pool — RF-only Hybrid network; messages never traverse the internet.
    RadioOnly,
    /// L pool — store-and-forward at a local RMS Relay "post office".
    PostOffice,
    /// MESH — Network Post Office (locally-run RMS Relay / AREDN mesh transport,
    /// normal CMS mail pool).
    Mesh,
    /// Peer-to-peer — direct station, no CMS, no creds, no routing flag.
    P2p,
}

/// GATED egress capability. EVERY method is an Agent-authority egress: the impl
/// gates it through [`guarded_egress`](tuxlink_security::guarded_egress) before
/// any connect/transmit happens, so a disarmed/expired/tainted/poisoned session
/// gets [`EgressPortError::Denied`] and NOTHING leaves the box. Object-safe so
/// [`crate::McpState`] can hold it as `Arc<dyn EgressPort>`.
#[async_trait]
pub trait EgressPort: Send + Sync {
    /// Connect to the configured CMS (Winlink common message server).
    async fn cms_connect(&self) -> Result<(), EgressPortError>;
    /// Verify the live CMS connection (a round-trip that touches the network).
    async fn verify_cms_connection(&self) -> Result<(), EgressPortError>;
    /// Connect the ARDOP modem to `target`.
    async fn ardop_connect(&self, target: String) -> Result<(), EgressPortError>;
    /// Run an ARDOP B2F message exchange with `target` for the given `intent`.
    async fn ardop_b2f_exchange(
        &self,
        target: String,
        intent: SessionIntentDto,
    ) -> Result<(), EgressPortError>;
    /// Run a VARA B2F message exchange with `target` for the given `intent`.
    async fn vara_b2f_exchange(
        &self,
        target: String,
        intent: SessionIntentDto,
    ) -> Result<(), EgressPortError>;
    /// Connect an AX.25 packet session to `call` over the optional digipeater
    /// `path`.
    async fn packet_connect(&self, call: String, path: Vec<String>)
        -> Result<(), EgressPortError>;
}

/// UNGATED pure-stop capability. Stopping a transmission/connection is ALWAYS
/// allowed — never gated by armed/taint state — because a working abort is a
/// safety primitive, not an egress. Returns [`PortError`] (operational failure
/// only; there is no authorization failure for an abort). Object-safe.
#[async_trait]
pub trait AbortPort: Send + Sync {
    /// Abort/disconnect the CMS connection.
    async fn cms_abort(&self) -> Result<(), PortError>;
    /// Disconnect the ARDOP modem.
    async fn ardop_disconnect(&self) -> Result<(), PortError>;
    /// Stop the active VARA session.
    async fn vara_stop_session(&self) -> Result<(), PortError>;
}

// ---------------------------------------------------------------------------
// Write + Compose (phase 3.4) — gated config/state writes + ungated drafting.
//
// WritePort methods MUTATE config/mailbox state and are gated like egress: the
// IMPL validates the agent-supplied input FIRST (a malformed value is rejected
// as `Invalid` WITHOUT consuming the armed grant), then runs the mutation
// through `guarded_egress(.., Agent, ..)`. So a disarmed/tainted session gets
// `Denied` and nothing is written; a bad input gets `Invalid` even when
// disarmed (validate-before-gate).
//
// ComposePort methods only STAGE a local outbox draft — no transmission happens
// until a later GATED connect — so they are UNGATED: they validate input but do
// NOT touch the guard and do NOT taint. They cannot return `Denied`.
// ---------------------------------------------------------------------------

/// Failure modes a write/compose port adapter can surface to the agent.
/// `Denied` is the egress-gate refusal (write tier only); `Invalid` is an
/// input-validation rejection (returned even when disarmed, before the gate);
/// `Failed` is an operational failure after both checks passed. The router maps
/// `Denied` onto an authorization-shaped error, `Invalid` onto
/// `invalid_request`, and `Failed` onto `internal_error`.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum WritePortError {
    /// The egress gate refused the Agent caller (unarmed / expired / tainted /
    /// poisoned). Carries the `EgressDenied` reason. Write tier only.
    #[error("denied: {0}")]
    Denied(String),
    /// The agent-supplied input failed validation BEFORE the gate. The session's
    /// armed grant is not consumed.
    #[error("invalid: {0}")]
    Invalid(String),
    /// The input was valid and the gate passed, but the operation itself failed.
    #[error("failed: {0}")]
    Failed(String),
}

impl From<ValidationError> for WritePortError {
    fn from(e: ValidationError) -> Self {
        WritePortError::Invalid(e.to_string())
    }
}

/// Narrow ARDOP write payload: just the operator-settable drive level.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, schemars::JsonSchema)]
pub struct ArdopWriteDto {
    /// Transmit drive level, `0..=100`.
    pub drive_level: u8,
}

/// Narrow VARA write payload: just the bandwidth in Hz (`500`/`2300`/`2750`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, schemars::JsonSchema)]
pub struct VaraWriteDto {
    /// VARA bandwidth in Hz; one of `500`, `2300`, `2750`.
    pub bandwidth_hz: u32,
}

/// Narrow packet (AX.25/KISS) write payload. Non-secret connection params only.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, schemars::JsonSchema)]
pub struct PacketWriteDto {
    /// Station SSID (`0..=15` by AX.25 convention; the impl range-checks).
    pub ssid: u8,
    /// KISS TNC TCP host.
    pub tcp_host: String,
    /// KISS TNC TCP port.
    pub tcp_port: u16,
    /// TX delay in milliseconds.
    pub txdelay_ms: u32,
}

/// A composed message draft to stage in the local outbox. Carries no secrets.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, schemars::JsonSchema)]
pub struct ComposeDraftDto {
    /// Primary recipient addresses.
    pub to: Vec<String>,
    /// Carbon-copy recipient addresses.
    pub cc: Vec<String>,
    /// Message subject.
    pub subject: String,
    /// Message body.
    pub body: String,
}

/// A form submission to stage in the local outbox.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, schemars::JsonSchema)]
pub struct SendFormDto {
    /// The catalog form id (e.g. `"ICS-213"`).
    pub form_id: String,
    /// The form's field name → value map.
    pub field_values: BTreeMap<String, String>,
    /// Primary recipient addresses.
    pub to: Vec<String>,
    /// Carbon-copy recipient addresses.
    pub cc: Vec<String>,
    /// The sender's callsign.
    pub senders_callsign: String,
    /// The sender's grid square.
    pub grid_square: String,
}

/// A GRIB weather-product request to stage in the local outbox. `lat`/`lon` are
/// `f64`, so this derives `PartialEq` but not `Eq`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, schemars::JsonSchema)]
pub struct GribRequestDto {
    /// Request center latitude.
    pub lat: f64,
    /// Request center longitude.
    pub lon: f64,
    /// Request mode/product selector (impl-interpreted).
    pub mode: String,
    /// Subject line for the staged request message.
    pub subject: String,
}

/// GATED config/state writes. EVERY method validates the agent-supplied input
/// first (returning [`WritePortError::Invalid`] without consuming the armed
/// grant) and then gates the mutation through
/// [`guarded_egress`](tuxlink_security::guarded_egress), so a
/// disarmed/expired/tainted/poisoned session gets [`WritePortError::Denied`] and
/// NOTHING is written. Object-safe so [`crate::McpState`] can hold it as
/// `Arc<dyn WritePort>`.
#[async_trait]
pub trait WritePort: Send + Sync {
    /// Set the ARDOP drive level.
    async fn set_ardop(&self, dto: ArdopWriteDto) -> Result<(), WritePortError>;
    /// Set the VARA bandwidth.
    async fn set_vara(&self, dto: VaraWriteDto) -> Result<(), WritePortError>;
    /// Set the packet (AX.25/KISS) connection params.
    async fn set_packet(&self, dto: PacketWriteDto) -> Result<(), WritePortError>;
    /// Set the station grid square.
    async fn set_grid(&self, grid: String) -> Result<(), WritePortError>;
    /// Set the position source (e.g. `"gps"` / `"manual"`).
    async fn set_position_source(&self, source: String) -> Result<(), WritePortError>;
    /// Set the GPS privacy: broadcast state + precision.
    async fn set_privacy(&self, gps_state: String, precision: String)
        -> Result<(), WritePortError>;
    /// Enable/disable packet listen mode.
    async fn set_packet_listen(&self, enabled: bool) -> Result<(), WritePortError>;
    /// Move a message between folders.
    async fn mailbox_move(
        &self,
        from: String,
        to: String,
        id: String,
    ) -> Result<(), WritePortError>;
    /// Save an attachment to a (validated) destination, returning the saved path.
    async fn attachment_save(
        &self,
        folder: String,
        id: String,
        filename: String,
        dest: String,
    ) -> Result<String, WritePortError>;
}

/// UNGATED compose/staging capability. EVERY method validates input but only
/// stages a LOCAL outbox draft — no transmission happens until a later GATED
/// connect — so it never touches the egress guard and never taints. It returns
/// the staged message id (MID) on success, or [`WritePortError::Invalid`] /
/// [`WritePortError::Failed`] (never `Denied`). Object-safe.
#[async_trait]
pub trait ComposePort: Send + Sync {
    /// Stage a composed message; returns the staged MID.
    async fn message_send(&self, dto: ComposeDraftDto) -> Result<String, WritePortError>;
    /// Stage a form submission; returns the staged MID.
    async fn send_form(&self, dto: SendFormDto) -> Result<String, WritePortError>;
    /// Stage a catalog inquiry for the given catalog item ids; returns the MID.
    async fn catalog_send_inquiry(&self, item_ids: Vec<String>)
        -> Result<String, WritePortError>;
    /// Stage a GRIB weather-product request; returns the staged MID.
    async fn grib_send_request(&self, dto: GribRequestDto) -> Result<String, WritePortError>;
}
