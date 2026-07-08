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

/// Non-secret radio-level rig (CAT) config — the hamlib model, the rigctld
/// endpoint, the CAT serial, and the close-serial/live-vfo/qsy behavior flags.
/// Shared by ARDOP + VARA (it is `Config.rig`, not per-modem). No secrets.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RigConfigDto {
    /// Hamlib rig model id for rigctld-based QSY/VFO control; `None` = no rig.
    pub rig_hamlib_model: Option<u32>,
    /// Host where rigctld listens.
    pub rigctld_host: String,
    /// TCP port rigctld listens on.
    pub rigctld_port: u16,
    /// rigctld binary name or path.
    pub rigctld_binary: String,
    /// Close the CAT serial before audio (internal-codec radios that share one
    /// serial between CAT and audio PTT).
    pub close_serial_sequencing: bool,
    /// Poll the VFO frequency from rigctld in real time.
    pub live_vfo_poll: bool,
    /// Walk ranked candidate frequencies on a connect failure (QSY).
    pub qsy_on_fail: bool,
    /// CAT serial device path for QSY/VFO control; `None` until the operator
    /// picks a port.
    pub cat_serial_path: Option<String>,
    /// CAT serial baud.
    pub cat_baud: u32,
}

/// Live + configured rig status. The live fields (`vfo_hz`, `mode`, `ptt`) are
/// `Option` because a best-effort transient rigctld read can fail (rig
/// unconfigured, rigctld absent, or the CAT serial busy with an active
/// session); on any such failure they are `None` while `configured` still
/// reports whether rig control is set up at all. NEVER carries a transmit
/// side effect — the probe behind it is CAT-read-only.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RigStatusDto {
    /// Live VFO frequency in Hz, when the transient read succeeded.
    pub vfo_hz: Option<u64>,
    /// Live mode token (e.g. `"PKTUSB"`), when known.
    pub mode: Option<String>,
    /// Live PTT state, when the transient read succeeded.
    pub ptt: Option<bool>,
    /// Whether rig control is configured (a hamlib model + CAT serial are set),
    /// independent of whether the live read succeeded.
    pub configured: bool,
}

/// One QSY (frequency-walk) candidate the agent can supply on a gated
/// connect/exchange: a dial `target` plus the frequency to tune for it. Mirrors
/// the monolith's `DialCandidate` field-for-field (snake_case wire form). An
/// omitted/empty candidate list reproduces today's single-dial behavior.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, schemars::JsonSchema)]
pub struct QsyCandidateDto {
    /// The dial target (station/gateway callsign) for this candidate.
    pub target: String,
    /// The frequency in Hz to tune before dialing this candidate; `None` skips
    /// the pre-audio CAT tune for it.
    pub freq_hz: Option<u64>,
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

/// One USB audio card, resolved to the identity fields the agent needs to
/// disambiguate look-alike devices (tuxlink-77seh, Contract 4). VID:PID + bus
/// path split two identically-named cards; `in_use` flags a card another program
/// currently holds. The agent applies the disambiguation METHOD (served as the
/// `tuxlink://playbook/audio-setup` guidance resource) — the code never ranks.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AudioCardDto {
    /// Human label from the card longname (e.g. `"C-Media USB Audio Device"`).
    pub name: String,
    /// The ALSA `plughw:CARD=<id>,DEV=0` name.
    pub alsa_name: String,
    /// Live boot-order `card<N>` index.
    pub card_index: u32,
    /// USB `vid:pid` (e.g. `"0d8c:013a"`); `None` for onboard/non-USB cards.
    pub vid_pid: Option<String>,
    /// sysfs USB device-node / bus path (e.g. `".../usb2/2-1"`) — distinguishes
    /// two identical-name cards on different ports. `None` when unresolved.
    pub bus_path: Option<String>,
    /// True when another program currently holds a capture or playback substream
    /// of this card (best-effort read of `/proc/asound/card<N>` status).
    pub in_use: bool,
}

/// Capture + playback audio device names for modem audio selection, plus the
/// richer per-card inspection list (`cards`, tuxlink-77seh) carrying VID:PID /
/// bus path / in-use for disambiguation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AudioDevicesDto {
    pub capture: Vec<String>,
    pub playback: Vec<String>,
    #[serde(default)]
    pub cards: Vec<AudioCardDto>,
}

/// A CUPS print destination (tuxlink-z2nwx, Contract 3), from `lpstat -p -d`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PrinterDto {
    /// The CUPS queue name passed to `lp -d <name>`.
    pub name: String,
    /// True for the system default destination (`lpstat -d`).
    pub is_default: bool,
}

/// Live backend (CMS connection / engine) status.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BackendStatusDto {
    pub connected: bool,
    pub transport: String,
    pub state: String,
}

/// Live modem status (tuxlink-7ppfq, Contract 2). Reports BOTH what is actually
/// `running` (live sessions) and what the operator has `selected` (their target),
/// with `kind` dispatched on the source of truth — never a hardcoded literal.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModemStatusDto {
    /// The PRIMARY running modem's kind (`"ardop"` / `"vara-hf"`), or `"idle"`
    /// when nothing is running. Dispatched on `running`, NEVER on `selected` —
    /// a `selected` fallback would re-introduce a false-positive against
    /// `connected`. When more than one modem runs, this is `running[0]` (a
    /// fixed tie-break; consult `running` + `conflict` for the full picture).
    pub kind: String,
    /// Whether the PRIMARY running modem is in a connected/open state. Pairs
    /// with `kind` (never with `selected`), so it is honest for the reported kind.
    pub connected: bool,
    /// The primary running modem's state string, or `"idle"` when nothing runs.
    pub state: String,
    /// Every live modem session (ARDOP and VARA are independent objects, so both
    /// can be non-idle). Empty when nothing is running. `SocketLost` counts as
    /// running (degraded) so the agent knows to close+reopen, not "idle".
    #[serde(default)]
    pub running: Vec<RunningModemDto>,
    /// The operator's persisted selected connection (their target), independent
    /// of what is live. Reported separately from `kind`/`running`.
    #[serde(default)]
    pub selected: Option<SelectedConnectionDto>,
    /// True when more than one modem is running — a state convention forbids but
    /// the code does not enforce; surfaced honestly so the agent can react.
    #[serde(default)]
    pub conflict: bool,
}

/// One live modem session within [`ModemStatusDto::running`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RunningModemDto {
    /// `"ardop"` or `"vara-hf"`.
    pub kind: String,
    /// The session's current state string.
    pub state: String,
}

/// The operator's selected connection, mirrored from `Config.active_connection`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SelectedConnectionDto {
    pub session_type: String,
    pub protocol: String,
}

/// Live VARA modem status.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VaraStatusDto {
    pub connected: bool,
    pub bandwidth: u32,
    pub state: String,
    /// Command-port (8300) reachability, classified WITHOUT disturbing a live
    /// session: `Some(true)` = the cmd port answered (or a session is Open),
    /// `Some(false)` = no answer, `None` = unknown (the session lock was
    /// contended, so the probe was skipped rather than made to wait).
    /// **cmd-reachable is NOT "ready to send"** — 8300 can accept while 8301
    /// (data) still lags on a WINE restart.
    pub reachable: Option<bool>,
}

/// Result of the read-only VARA deep probe (`vara_probe`): connect the cmd port
/// and read the startup banner / `VERSION` reply to distinguish "nothing there"
/// from "something is listening but is not VARA" from "a real VARA answered".
/// Read-only — never sends a stateful setter, never opens the data port.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VaraProbeDto {
    /// `"down"` (no TCP), `"socket-not-vara"` (answered but not VARA), or
    /// `"vara-ok"` (a real VARA banner / VERSION reply).
    pub classification: String,
    /// The trimmed banner / VERSION reply text, when any bytes were read.
    pub banner: Option<String>,
}

/// One checkpoint of the VARA-under-WINE install pipeline
/// (deps → prefix → vara → vb6 → ocx → verify → autostart), curated from the
/// setup engine's JSONL `checkpoint` events. All fields are `Option` because
/// the engine's `hello` / `checkpoint` / `summary` lines carry different subsets.
/// App-owned provisioning telemetry — no external untrusted content.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, schemars::JsonSchema)]
pub struct VaraCheckpointDto {
    /// Stable checkpoint id (e.g. `"deps"`, `"prefix"`, `"vara"`, `"verify"`).
    pub id: Option<String>,
    /// 1-based position of this checkpoint in the pipeline.
    pub index: Option<u32>,
    /// Total number of checkpoints in the pipeline.
    pub total: Option<u32>,
    /// Checkpoint state token (e.g. `"running"`, `"ok"`, `"failed"`).
    pub state: Option<String>,
    /// Human-readable detail line for display / diagnosis.
    pub detail: Option<String>,
}

/// Result of the read-only, offline VARA install-readiness probe: whether VARA
/// is provisioned (`ready`) plus each pipeline checkpoint's state. Never
/// launches VARA and never touches the network.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, schemars::JsonSchema)]
pub struct VaraInstallStatusDto {
    /// True iff the setup engine reported the core checkpoints green.
    pub ready: bool,
    /// Per-checkpoint state from the status stream, for display.
    pub checkpoints: Vec<VaraCheckpointDto>,
}

/// Terminal summary of a VARA install run: whether it completed green (`ok`),
/// the WINE prefix it provisioned into, and the VARA version label reported by
/// the engine.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, schemars::JsonSchema)]
pub struct VaraInstallSummaryDto {
    /// True iff the install completed all checkpoints successfully.
    pub ok: bool,
    /// The WINE prefix VARA was installed into, when known.
    pub prefix: Option<String>,
    /// The installed VARA version label, when known.
    pub vara_version: Option<String>,
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
// Station-intelligence DTOs (phase 3.2 / Chunk 1). All Tier-1 reads; the
// `find_stations` / `predict_path` / `solar_conditions` tools are INERT — they
// call the port and JSON-encode the result, never touching the egress guard
// (no taint, no gate). The agent-supplied INPUT dtos carry `schemars::JsonSchema`
// so rmcp can advertise their tool-input schema.
//
// Curate-down notes baked into the shapes:
// - `GatewayDto` deliberately omits sysop_name/email/homepage: PII + a prompt-
//   injection surface the agent should never see.
// - `PredictRequestDto` carries NO tx_grid: the operator's own grid is injected
//   by the Chunk-2 monolith impl, never agent-supplied (a malicious agent must
//   not be able to spoof the station's location into a prediction).
// ---------------------------------------------------------------------------

/// A Winlink RMS gateway operating mode / transport. Kebab-case on the wire so
/// the agent-facing values read `vara-hf`, `ardop-hf`, `robust-packet`, etc.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum StationModeDto {
    VaraHf,
    Packet,
    ArdopHf,
    Pactor,
    RobustPacket,
}

/// A gateway's antenna type, used as an optional prediction parameter. Lowercase
/// on the wire (`beam` / `dipole` / `vertical`).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum GatewayAntennaDto {
    Beam,
    Dipole,
    Vertical,
}

/// Agent-supplied filter for [`StationPort::find_stations`]. `modes` and `bands`
/// are AND-ish narrowing selectors; `history_hours` bounds how far back a
/// gateway must have been last heard. Empty `modes`/`bands` mean "no filter".
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, schemars::JsonSchema)]
pub struct StationFilterDto {
    /// Restrict to these transports; empty means all transports.
    #[serde(default)]
    pub modes: Vec<StationModeDto>,
    /// Only gateways heard within this many hours; `None` means no bound.
    #[serde(default)]
    pub history_hours: Option<u32>,
    /// Restrict to these amateur bands (e.g. `"40m"`); empty means all bands.
    #[serde(default)]
    pub bands: Vec<String>,
}

/// One curated RMS gateway directory entry. Public directory data, no PII:
/// deliberately NO sysop name / email / homepage (see module note).
///
/// **Structured-only.** Untrusted free-text directory fields (`location`,
/// `last_update`) are intentionally OMITTED: they are agent-facing
/// prompt-injection surfaces with no structured contract. A future follow-up
/// re-adds a PARSED `last_update_ms: Option<u64>`; the raw free-text never
/// returns. The remaining fields are either app-controlled enums (`mode`,
/// `antenna`), numeric (`frequencies_khz`), or validated by the impl (`callsign`
/// shape-checked, bogus entries dropped; `grid` Maidenhead-validated or nulled;
/// `channel` control-stripped + length-capped).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GatewayDto {
    pub mode: StationModeDto,
    /// The Winlink "channel" identifier (e.g. a frequency/mode channel name).
    pub channel: String,
    pub callsign: String,
    /// Maidenhead grid locator, when known and structurally valid. Set to `None`
    /// by the impl when the directory value fails Maidenhead validation.
    pub grid: Option<String>,
    /// Dial frequencies in kHz this channel advertises.
    pub frequencies_khz: Vec<f64>,
    /// Gateway antenna type, when known.
    pub antenna: Option<GatewayAntennaDto>,
    /// Great-circle distance in km from the operator's grid to this gateway. `None` when the
    /// gateway grid is absent/invalid OR the operator grid is unresolved.
    pub distance_km: Option<f64>,
    /// Same distance in statute miles (km * 0.621371). Served alongside km so the agent never
    /// does unit math (US/miles-preferred audience; global toggle tracked in tuxlink-25l40).
    pub distance_mi: Option<f64>,
    /// Great-circle initial bearing in degrees [0,360) from the operator to this gateway.
    /// `None` when distance is unknown OR zero. (Sibling `PathPredictionDto`'s `bearing_deg`
    /// is non-optional; the asymmetry is intentional — gateway grids can be absent.)
    pub bearing_deg: Option<f64>,
}

/// Output of [`StationPort::find_stations`]: the matched gateways plus a fetch
/// timestamp the agent reasons freshness from.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StationListDto {
    pub gateways: Vec<GatewayDto>,
    /// When the underlying directory was fetched (unix ms), when known. The agent
    /// reasons freshness directly from this stamp; there is no separate
    /// cache-provenance flag.
    pub fetched_at_ms: Option<u64>,
    /// The operator's own 4-char grid used to compute per-gateway distances (provenance).
    /// `None` when unresolved — lets the agent explain why all distances are null.
    pub operator_grid: Option<String>,
}

/// Agent-supplied request for [`PredictionPort::predict_path`]. Carries NO
/// `tx_grid`: the operator's own grid is injected by the Chunk-2 impl, never
/// agent-supplied (see module note).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, schemars::JsonSchema)]
pub struct PredictRequestDto {
    /// The TARGET (receiving) station's Maidenhead grid locator.
    pub rx_grid: String,
    /// Candidate dial frequencies in kHz to predict across.
    pub frequencies_khz: Vec<f64>,
    /// The target gateway's antenna type, when known (refines the prediction).
    #[serde(default)]
    pub gateway_antenna: Option<GatewayAntennaDto>,
}

/// Per-channel hourly HF reliability prediction. Each vector is 24 entries long,
/// indexed by UTC hour `0..=23`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChannelReliabilityDto {
    pub frequency_khz: f64,
    /// Reliability `0.0..=1.0` per UTC hour (24-long).
    pub rel_by_hour: Vec<f64>,
    /// Predicted SNR (dB) per UTC hour (24-long).
    pub snr_by_hour: Vec<f64>,
    /// MUF-day fraction per UTC hour (24-long).
    pub mufday_by_hour: Vec<f64>,
}

/// A full path prediction from the operator's station to the target grid.
/// `tx_grid` is the operator's own 4-char grid, injected by the impl as
/// provenance (it is NOT agent-supplied).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PathPredictionDto {
    pub bearing_deg: f64,
    pub distance_km: f64,
    /// Smoothed sunspot number used for the prediction.
    pub ssn: f64,
    pub year: i32,
    pub month: u8,
    /// The operator's own 4-char grid (provenance; injected by the impl).
    pub tx_grid: String,
    pub channels: Vec<ChannelReliabilityDto>,
}

/// A current space-weather snapshot. The numeric indices are `Option` because a
/// stale/offline source may not carry all of them; `ssn` is always present (it
/// is the value predictions key off).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SolarSnapshotDto {
    /// Solar flux index (10.7 cm), when known.
    pub sfi: Option<f64>,
    /// Geomagnetic A index, when known.
    pub a_index: Option<f64>,
    /// Geomagnetic K index, when known.
    pub k_index: Option<f64>,
    /// Sunspot number used for predictions.
    pub ssn: f64,
    /// When this snapshot was last updated (unix ms).
    pub updated_at_ms: u64,
    /// Provenance of the data (e.g. `"bundled"`, `"noaa"`).
    pub source: String,
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
    /// Read-only deep probe of the VARA cmd port (banner / VERSION). Never
    /// sends a stateful setter, never opens the data port, never transmits.
    async fn vara_probe(&self) -> Result<VaraProbeDto, PortError>;
    async fn position_status(&self) -> Result<PositionStatusDto, PortError>;
    async fn platform_info(&self) -> Result<PlatformInfoDto, PortError>;
    async fn wizard_completed(&self) -> Result<bool, PortError>;
    /// Whether a stored P2P peer password is Set or NotSet for `callsign`.
    /// Returns the boolean only — never the password — so this is NOT a taint
    /// source.
    async fn p2p_peer_password_status(&self, callsign: &str) -> Result<bool, PortError>;
    /// Report the rig's configured state and, best-effort, its live VFO
    /// frequency / mode / PTT via a transient CAT read. NEVER transmits; the
    /// live fields are `None` when the read fails (unconfigured / rigctld
    /// absent / serial busy).
    async fn rig_status(&self) -> Result<RigStatusDto, PortError>;
}

/// VARA-under-WINE provisioning (tuxlink-w7212). The two probes are read-only
/// and do NOT taint (app-owned build/readiness data). `vara_install_start` runs
/// the one-time, prep-time install of VARA HF; it is **NON-TRANSMIT** (it drives
/// `apt`/`winetricks`/`wine` to install software and never keys a radio), so it
/// is NOT routed through the transmit consent gate — the operator-presence guard
/// is the engine's own `pkexec` OS password prompt. Object-safe so
/// [`crate::McpState`] can hold it as `Arc<dyn ProvisionPort>`.
#[async_trait]
pub trait ProvisionPort: Send + Sync {
    /// True iff the VARA setup engine is bundled in this build. Read-only.
    async fn vara_engine_available(&self) -> Result<bool, PortError>;
    /// Offline readiness probe (no network, no launch). Read-only.
    async fn vara_install_status(&self) -> Result<VaraInstallStatusDto, PortError>;
    /// Install VARA HF from a user-supplied installer `.exe` path. NON-TRANSMIT;
    /// gated only by pkexec's OS password prompt, not the transmit consent gate.
    async fn vara_install_start(
        &self,
        installer_path: String,
    ) -> Result<VaraInstallSummaryDto, PortError>;
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
    /// Read the non-secret radio-level rig (CAT) config. Read-only; no secrets.
    async fn rig(&self) -> Result<RigConfigDto, PortError>;
}

/// Local host capabilities (tuxlink-z2nwx, Contract 3): hardware device
/// enumeration (read-only, none taint) PLUS the shell-equivalent local actions
/// of printing and report export. None of these are RADIO-1 acts or external
/// egress — they are ungated, exactly what a competent operator could do at a
/// shell (list printers, `lp` a file, write a report to their Documents folder).
#[async_trait]
pub trait DevicePort: Send + Sync {
    async fn serial(&self) -> Result<Vec<SerialDeviceDto>, PortError>;
    async fn bluetooth(&self) -> Result<Vec<BluetoothDeviceDto>, PortError>;
    async fn audio(&self) -> Result<AudioDevicesDto, PortError>;
    /// Enumerate CUPS print destinations (`lpstat -p -d`). Empty list when CUPS
    /// is absent — the agent falls back to `export_report`.
    async fn printer_list(&self) -> Result<Vec<PrinterDto>, PortError>;
    /// Print a local file to a CUPS destination (`lp -d <printer> <path>`). An
    /// ungated local action; not a transmission. CUPS auto-filters text/markdown.
    async fn print_document(&self, printer: String, path: String) -> Result<(), PortError>;
    /// Write agent-generated markdown/text to a sandboxed reports directory
    /// (`~/Documents/Tuxlink/reports/`). The agent picks the FILENAME, never the
    /// directory; `..`/absolute/traversal paths are rejected. Returns the
    /// absolute path written.
    async fn export_report(&self, filename: String, content: String) -> Result<String, PortError>;
}

/// Session-log snapshot. The snapshot can carry untrusted wire content → the
/// calling tool taints.
#[async_trait]
pub trait LogPort: Send + Sync {
    /// Snapshot the current session log. **TAINT** (may contain untrusted wire
    /// content even after sink redaction).
    async fn snapshot(&self) -> Result<Vec<LogLineDto>, PortError>;
}

/// Winlink RMS gateway directory lookups. Public directory data, cached. Does
/// NOT taint (app-owned/public content) and is NOT gated (read-only; never
/// transmits).
#[async_trait]
pub trait StationPort: Send + Sync {
    /// List RMS gateways matching `filter`. Read-only; does not taint or gate.
    async fn find_stations(&self, filter: StationFilterDto)
        -> Result<StationListDto, PortError>;
}

/// Offline HF propagation prediction + space-weather reads. Both methods are
/// read-only computation/data reads: they do NOT taint and are NOT gated (no
/// transmission).
#[async_trait]
pub trait PredictionPort: Send + Sync {
    /// Predict the HF path from the operator's station to the requested target
    /// grid across the requested dials. Read-only; does not taint or gate.
    async fn predict_path(&self, req: PredictRequestDto) -> Result<PathPredictionDto, PortError>;
    /// Report the current space-weather snapshot. Read-only; does not taint or
    /// gate.
    async fn solar(&self) -> Result<SolarSnapshotDto, PortError>;
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
    /// Tune the rig to `freq_hz` (set VFO + the HF data mode) over CAT. This
    /// COMMANDS the radio and is therefore EGRESS, in the SAME authority class
    /// as a transmit: a disarmed / expired / tainted / poisoned session is
    /// `Denied` and nothing is sent to the radio. (`rig_tune` takes only a
    /// single frequency — a bare tune has no candidate walk.)
    async fn rig_tune(&self, freq_hz: u64) -> Result<(), EgressPortError>;
    /// Connect the ARDOP modem to `target`. `freq_hz` (when `Some`) is the
    /// pre-audio CAT tune for the single dial; `qsy_candidates` (when `Some` +
    /// non-empty) overrides `target`/`freq_hz` with an ordered frequency walk
    /// (operator-gated). `None`/empty reproduces the legacy single dial.
    async fn ardop_connect(
        &self,
        target: String,
        freq_hz: Option<u64>,
        qsy_candidates: Option<Vec<QsyCandidateDto>>,
    ) -> Result<(), EgressPortError>;
    /// Run an ARDOP B2F message exchange with `target` for the given `intent`.
    /// No `freq_hz` / `qsy_candidates`: the ARDOP lifecycle tunes at the CONNECT
    /// (via [`EgressPort::ardop_connect`]'s dial walk), and the B2F exchange runs
    /// over the ALREADY-connected link — a pre-tune is genuinely N/A here, so
    /// accepting one would be an inert, misleading transmit-adjacent param.
    async fn ardop_b2f_exchange(
        &self,
        target: String,
        intent: SessionIntentDto,
    ) -> Result<(), EgressPortError>;
    /// Run a VARA B2F message exchange with `target` for the given `intent`.
    /// VARA differs from ARDOP: its B2F connects + tunes + exchanges in a single
    /// call, so `freq_hz` / `qsy_candidates` are live here (same pre-tune + QSY
    /// semantics as [`EgressPort::ardop_connect`]).
    async fn vara_b2f_exchange(
        &self,
        target: String,
        intent: SessionIntentDto,
        freq_hz: Option<u64>,
        qsy_candidates: Option<Vec<QsyCandidateDto>>,
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

// ---------------------------------------------------------------------------
// Outbox read port — operator-UI only; never exposed as an agent #[tool].
// ---------------------------------------------------------------------------

/// One staged outbox record as seen by the operator confirm surface.
///
/// v1 carries no `staged_by` provenance field — there is no marker infra in
/// this release. A provenance marker is a filed follow-up (M3 resolution).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, schemars::JsonSchema)]
pub struct StagedRecordDto {
    /// The Winlink message-ID (MID) assigned when the message was staged.
    pub mid: String,
    /// Primary recipient addresses.
    pub to: Vec<String>,
    /// Carbon-copy recipient addresses.
    pub cc: Vec<String>,
    /// Message subject.
    pub subject: String,
    /// Decoded plain-text body.
    pub body: String,
}

/// Non-tainting read of the local outbox — returns the staged set exactly as
/// the operator will see it in the confirm surface. **Never exposed as an
/// agent `#[tool]`**; reached only by the operator-driven `outbox_staged_list`
/// Tauri command (Task 8b). Calling this method does NOT mark messages read
/// and does NOT touch the egress guard.
#[async_trait]
pub trait OutboxReadPort: Send + Sync {
    async fn list_staged(&self) -> Result<Vec<StagedRecordDto>, PortError>;
}
