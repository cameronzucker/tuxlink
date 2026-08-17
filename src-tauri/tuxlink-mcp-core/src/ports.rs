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
    /// The CALLER's input was malformed or refused (unparseable JSON in an
    /// opaque-string argument like `args_json`, a routine name that would
    /// escape the store directory, …). The router surfaces this as an
    /// invalid-request tool error — the agent can fix its input and retry —
    /// never as an internal error, which would mis-signal a server bug.
    #[error("invalid input: {0}")]
    InvalidInput(String),
    /// The caller is not authorized for this READ (arm/taint gate). Carries
    /// the guard's denial reason; the router surfaces it in the same
    /// "not authorized …" shape as egress/write denials so client-side
    /// denial classifiers recognize it. `find_peers` is the one arm-gated
    /// read — its denial previously shipped as `Unavailable` and read as an
    /// outage instead of an authorization refusal (tuxlink-9n4cr).
    #[error("not authorized: {0}")]
    Denied(String),
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
    /// Display name (user folders may carry spaces/case, e.g. "ARES Drills").
    pub name: String,
    /// The folder reference `mailbox_list` / `message_read` / `mailbox_move`
    /// accept. Previously only the display name was returned, which those
    /// tools cannot consume — the round-trip either errored or ok-emptied
    /// (tuxlink-9n4cr). Always pass THIS, not `name`.
    pub slug: String,
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

/// One in-app documentation search hit. `slug` is the key `docs_read` takes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DocsHitDto {
    pub title: String,
    pub slug: String,
    pub snippet: String,
}

/// A whole documentation page, returned by `docs_read`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DocBodyDto {
    pub slug: String,
    pub title: String,
    pub body: String,
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

/// Non-secret packet (AX.25 / KISS) config. Unset / not-applicable fields are
/// `None` (null on the wire): a TCP KISS link has no serial `baud`, and a
/// never-configured link has no host/port. The previous shape emitted
/// `0`/`""` sentinels, which models read as configured values (surface-repair
/// ledger row 6).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PacketConfigDto {
    pub kiss_host: Option<String>,
    pub kiss_port: Option<u16>,
    pub baud: Option<u32>,
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
    /// of what is live. Reported separately from `kind`/`running`; its `note`
    /// field carries the same caveat on the wire (ledger row 2).
    #[serde(default)]
    pub selected: Option<SelectedConnectionDto>,
    /// The most recent completed/failed transient B2F session, when one has
    /// happened since app start (ledger row 2 — the observation surface's
    /// memory; sessions are transient and `kind`/`state` return to idle).
    #[serde(default)]
    pub last_session: Option<LastSessionSummaryDto>,
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

/// On-wire teaching for [`SelectedConnectionDto::note`] (ledger row 2): the
/// zqo run watched a model read a sticky `selected: vara-hf` as "VARA is
/// active" after an ARDOP session — the doc-comment semantics never cross the
/// wire, so the wire carries them itself (the `with_edit_protocol` precedent:
/// teach ON the wire, not only in tool descriptions).
pub const SELECTED_CONNECTION_NOTE: &str =
    "the operator's persisted target, NOT what is live - consult `running` for live sessions";

/// The operator's selected connection, mirrored from `Config.active_connection`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SelectedConnectionDto {
    pub session_type: String,
    pub protocol: String,
    /// Always [`SELECTED_CONNECTION_NOTE`] — self-description the reader
    /// actually sees. `#[serde(default)]` so older payloads still parse.
    #[serde(default)]
    pub note: String,
}

/// Summary of the most recent COMPLETED or FAILED OUTBOUND dial session
/// (ledger row 2): sessions are transient by design and `modem_get_status`
/// honestly reports idle afterward — this field is the memory of what just
/// happened, so an agent that ran a session (or is diagnosing one) does not
/// have to infer the outcome from an idle status. Scope: outbound dials
/// (agent- or operator-initiated) on every dial transport. Inbound
/// LISTENER exchanges are deliberately NOT recorded here — they already
/// surface through the contact observation records, and double-reporting
/// them would let an inbound call overwrite the memory of the dial the
/// agent just ran.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LastSessionSummaryDto {
    /// `"ardop"` / `"vara-hf"` / `"vara-fm"` / `"packet"` — the transport
    /// that ran the dial.
    pub transport: String,
    /// The dialed target (gateway callsign), when the seam knew it.
    pub target: Option<String>,
    /// `"completed"` (the B2F exchange returned Ok) or `"failed"`.
    pub outcome: String,
    /// The failure detail for `"failed"` outcomes (redacted seam text);
    /// absent on success.
    pub detail: Option<String>,
    /// When the session ended (unix ms).
    pub ended_at_ms: u64,
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

/// The classifier-weights job half of [`ClassifyWeightsStatusDto`]. Mirrors
/// the app's persistent job record (tuxlink-13ofm): one job at a time,
/// survives restart, resumes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, schemars::JsonSchema)]
pub struct ClassifyWeightsJobDto {
    /// `waiting` | `downloading` | `verifying` | `complete` | `failed`.
    pub state: String,
    /// Waiting reason / failure message, operator-phrased.
    pub detail: Option<String>,
    /// On `failed`: `network` (auto-retried), `source` (switch source or
    /// sideload), `digest-mismatch` (content refused by name), `io`,
    /// `cancelled`.
    pub error_class: Option<String>,
    /// File currently moving, when downloading/verifying.
    pub file: Option<String>,
    /// Files already digest-verified and installed by this job.
    pub files_done: Vec<String>,
    /// Where the bytes come from, as a display string.
    pub source: String,
    pub started_unix: u64,
    pub updated_unix: u64,
}

/// Result of the read-only classifier-weights probe: whether usable T1
/// weights exist on the search path, how strongly their content is vouched
/// for, and the state of the acquisition job if one exists.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, schemars::JsonSchema)]
pub struct ClassifyWeightsStatusDto {
    pub model_id: String,
    /// Whole-payload size in bytes (~134 MB for the primary model).
    pub total_bytes: u64,
    /// True iff a structurally-valid copy exists somewhere on the search path.
    pub ready: bool,
    /// Strongest integrity claim for the ready copy: `digest-pinned` (every
    /// byte verified against the release pins at acquisition), `size-verified`
    /// (manifest byte lengths match), or `structure`.
    pub integrity: Option<String>,
    /// Directory of the ready copy.
    pub location: Option<String>,
    /// One-line locator summary; when absent it names every path searched.
    pub summary: String,
    /// The version-matched default download base for this build.
    pub default_source: String,
    pub job: Option<ClassifyWeightsJobDto>,
}

/// One `[section] key = value` assignment for `vara_ini_apply`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, schemars::JsonSchema)]
pub struct VaraIniEditDto {
    /// INI section name without brackets, e.g. `Soundcard`.
    pub section: String,
    /// Key exactly as VARA writes it, e.g. `Output Device Name`.
    pub key: String,
    /// New value, verbatim. Single-line only; control characters are rejected.
    pub value: String,
}

/// Input for `vara_ini_apply` — the stop-edit-start VARA.ini configuration
/// bounce (tuxlink-iww9r).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, schemars::JsonSchema)]
pub struct VaraIniApplyDto {
    /// WINE prefix path; absent → the engine default
    /// (`~/.local/share/wine-vara/prefix`). A leading `~/` is expanded.
    pub prefix: Option<String>,
    /// `"primary"` (drive_c/"VARA HF" or drive_c/VARA) or `"vara2"`
    /// (drive_c/VARA2); absent → primary.
    pub instance: Option<String>,
    /// The assignments to write. May be empty only when `relaunch` is true
    /// (a pure bounce).
    pub edits: Vec<VaraIniEditDto>,
    /// Relaunch VARA after the edit and verify its cmd port. Default true.
    pub relaunch: Option<bool>,
}

/// Outcome of one `vara_ini_apply` call. Mirrors the app-crate report.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, schemars::JsonSchema)]
pub struct VaraIniApplyReportDto {
    /// Absolute path of the edited `VARA.ini`.
    pub ini_path: String,
    /// Path of the timestamped pre-edit backup; `None` when the INI was
    /// created fresh.
    pub backup_path: Option<String>,
    /// True when the INI did not exist and was created.
    pub created: bool,
    /// Number of edits applied.
    pub applied: usize,
    /// True when VARA was relaunched and its cmd port came up.
    pub relaunched: bool,
    /// The cmd port the post-edit config declares (`None` = unknowable for a
    /// portless second instance).
    pub cmd_port: Option<u16>,
}

/// Shared shape validation for `vara_ini_apply`, run BEFORE the egress gate
/// (an invalid payload is `Invalid` even when disarmed and never consumes the
/// armed grant). Checks: known instance selector; edits non-empty unless a
/// relaunch was requested; every edit single-line with a bracket-free section
/// and an `=`-free key (the INI layer writes verbatim, so an embedded newline
/// would inject lines and an `=` in a key would re-parse differently).
pub fn validate_vara_ini_apply(dto: &VaraIniApplyDto) -> Result<(), WritePortError> {
    match dto.instance.as_deref().map(str::trim) {
        None | Some("") | Some("primary") | Some("vara2") => {}
        Some(other) => {
            return Err(WritePortError::Invalid(format!(
                "unknown VARA instance {other:?}: expected \"primary\" or \"vara2\""
            )))
        }
    }
    if dto.edits.is_empty() && !dto.relaunch.unwrap_or(true) {
        return Err(WritePortError::Invalid(
            "nothing to do: no edits given and no relaunch requested".into(),
        ));
    }
    let flat = |s: &str| !s.chars().any(|c| c == '\r' || c == '\n' || c == '\0');
    for e in &dto.edits {
        if e.section.trim().is_empty() || !flat(&e.section) || e.section.contains(['[', ']']) {
            return Err(WritePortError::Invalid(format!(
                "invalid section name {:?}: must be non-empty, single-line, without brackets",
                e.section
            )));
        }
        if e.key.trim().is_empty() || !flat(&e.key) || e.key.contains('=') {
            return Err(WritePortError::Invalid(format!(
                "invalid key {:?}: must be non-empty, single-line, without '='",
                e.key
            )));
        }
        if !flat(&e.value) {
            return Err(WritePortError::Invalid(format!(
                "invalid value for {:?}: control characters/newlines would corrupt the INI",
                e.key
            )));
        }
    }
    Ok(())
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
    /// Where the line came from: `backend` (engine state), `transport`
    /// (link/session events), or `wire` (raw protocol text). Previously
    /// dropped at this boundary, leaving agents unable to tell a wire line
    /// from an app line when diagnosing (tuxlink-9n4cr).
    pub source: String,
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
/// the agent-facing values read `vara-hf`, `vara-fm`, `ardop-hf`,
/// `robust-packet`, etc.
///
/// `VaraFm` serializes as `"vara-fm"`, the SAME token the monolith
/// [`ListingMode::VaraFm`](crate) uses and the frontend `ListingMode` union
/// carries. VARA FM stations reach the agent with the same fidelity the frontend
/// gets (they are sourced from the channels JSON API, not a text `/listings/`
/// page).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum StationModeDto {
    VaraHf,
    VaraFm,
    Packet,
    ArdopHf,
    Pactor,
    RobustPacket,
}

/// Which VARA engine an agent egress dial should use. Mirrors the monolith's
/// `TransportKind::VaraHf` / `TransportKind::VaraFm` split (Task 4); `None` at
/// the call site maps to [`VaraEngineDto::VaraHf`] (backward-compatible with
/// every existing caller). Take this from the target peer channel's
/// `transport` field — do not guess.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum VaraEngineDto {
    VaraHf,
    VaraFm,
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
    /// Restrict to gateways offering a channel at one of these occupied
    /// bandwidths in Hz. Only the fixed classes `500`, `2300`, and `2750` are
    /// classified; a channel with any OTHER bandwidth (e.g. ARDOP 1000/2000) or
    /// no reported bandwidth passes every bandwidth filter, and a gateway stays
    /// if any of its channels passes. `None` or empty means no bandwidth filter.
    #[serde(default)]
    pub bandwidths: Option<Vec<u32>>,
    /// When `Some(true)`, corroborate each gateway against the operator's recent
    /// FT-8 decodes and stamp `ft8_corroborated` per gateway plus `evidence`
    /// params on the result. Requires the FT-8 listener to be available; the
    /// request is refused with an unavailable error otherwise. `None`/`false`
    /// serves gateways without evidence (every `ft8_corroborated` stays null).
    #[serde(default)]
    pub ft8_evidence: Option<bool>,
    /// SNR floor in dB for a decode to count as evidence when `ft8_evidence` is
    /// set. `None` uses the default floor (-24 dB). Ignored when `ft8_evidence`
    /// is not requested.
    #[serde(default)]
    pub ft8_snr_min_db: Option<i32>,
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
    /// Per-channel detail from the Winlink gateway channels JSON API (dial
    /// frequency, occupied bandwidth, mode, operating hours), when the gateway is
    /// present in the channels feed. Empty when it is not; the bare
    /// `frequencies_khz` above is then the only dial data. `#[serde(default)]` so
    /// a payload from a pre-Task-12 producer (or an older cached/fixture DTO)
    /// still deserializes.
    #[serde(default)]
    pub channels: Vec<ChannelDto>,
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
    /// `Some(true)` when recent FT-8 decodes corroborate this gateway is
    /// reachable, `Some(false)` when evidence was evaluated but did not
    /// corroborate it, `None` when FT-8 evidence was not requested (see
    /// [`StationFilterDto::ft8_evidence`]). `#[serde(default)]` for
    /// pre-Task-12 / cached-DTO backward-compat.
    #[serde(default)]
    pub ft8_corroborated: Option<bool>,
}

/// One per-channel row from the Winlink gateway channels JSON API, curated onto
/// a [`GatewayDto`]. Mirrors the monolith `ChannelDetail` (dial frequency in
/// kHz, occupied bandwidth in Hz when the mode implies one, the transport token,
/// and the advertised operating hours).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChannelDto {
    /// Dial frequency in kHz (offset-free; the API reports the dial directly).
    pub frequency_khz: f64,
    /// Occupied bandwidth in Hz when the mode implies a fixed one (VARA
    /// 500/2300/2750, ARDOP 500/1000/2000); `None` for a mode with no fixed
    /// bandwidth (VARA FM, Packet, Pactor, Robust Packet).
    pub bandwidth_hz: Option<u32>,
    /// The transport this channel runs (`vara-hf`, `vara-fm`, `ardop-hf`,
    /// `packet`, `pactor`, `robust-packet`).
    pub mode: String,
    /// The gateway's advertised operating hours for this channel, e.g. `"00-23"`.
    pub operating_hours: Option<String>,
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
    /// The FT-8 evidence parameters this result was corroborated under, present
    /// only when [`StationFilterDto::ft8_evidence`] was requested. Lets the agent
    /// explain the corroboration (SNR floor, recency window, radius model) and
    /// which bands actually carried qualifying decodes. `#[serde(default)]` for
    /// pre-Task-12 / cached-DTO backward-compat.
    #[serde(default)]
    pub evidence: Option<EvidenceParamsDto>,
}

/// The FT-8 evidence-corroboration parameters a [`StationListDto`] was computed
/// under (provenance for the `ft8_corroborated` stamps). Present only when
/// evidence was requested.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EvidenceParamsDto {
    /// The SNR floor in dB a decode had to clear to count as evidence.
    pub snr_min_db: i32,
    /// The recency window in ms: a decode older than this could not corroborate.
    pub recency_ms: u64,
    /// The decode-distance-to-radius scale factor (radius = factor × operator
    /// distance, clamped to `[radius_min_mi, radius_max_mi]`).
    pub radius_factor: f64,
    /// The corroboration-radius floor in miles.
    pub radius_min_mi: f64,
    /// The corroboration-radius cap in miles.
    pub radius_max_mi: f64,
    /// The bands that carried at least one qualifying decode in-window
    /// (first-occurrence order), independent of any gateway match.
    pub sampled_bands: Vec<String>,
}

/// One curated RF-reachability observation on a peer (spec §2). Structured-only:
/// callsigns are sanitizer-floored by the impl (bogus tokens dropped); no
/// free-text crosses.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PeerChannelDto {
    /// `"packet"` | `"ardop"` | `"vara-hf"` | `"vara-fm"`.
    pub transport: String,
    pub target_callsign: String,
    pub via: Vec<String>,
    pub freq_hz: Option<u64>,
    /// `"incoming"` | `"outgoing"`.
    pub direction: String,
    pub ok: u32,
    pub fail: u32,
    pub last_seen: String,
    /// Who authored the row: `"observed"` (the recorder, from a real
    /// concluded attempt) | `"manual"` (operator-entered in the contact
    /// editor) | `"unknown"` (future-binary quarantine). Defaults to
    /// `"observed"` when absent — a pre-`source` server means every row it
    /// serves was recorder-written (tuxlink-f0th0).
    #[serde(default = "default_peer_channel_source")]
    pub source: String,
}

/// serde default for [`PeerChannelDto::source`]: a payload from a
/// pre-`source` server carries only recorder-written rows.
fn default_peer_channel_source() -> String {
    "observed".to_string()
}

/// One curated peer-station roster entry — since the contacts-superset pivot
/// (spec §AMENDMENT), a row is a CONTACT with reachability. A CURATION, not a
/// DTO mirror [R2-S1]: free text (name, notes, email) is DROPPED on purpose
/// [R2-S11][R4-9], every callsign is sanitizer-floored by the impl, and
/// **telnet endpoint data never crosses the agent surface under ANY arm
/// state** (spec §AMENDMENT pt. 6: the agent cannot dial telnet, so it has no
/// use for an address).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PeerDto {
    pub id: String,
    /// The exact SSID-bearing callsign — the contact's primary identity and
    /// the wire target of any dial [R3-9].
    pub callsign: String,
    /// `"confirmed"` | `"unconfirmed"` | `"unknown"` — CURATION tier, not
    /// identity authentication (anyone can transmit any callsign).
    pub tier: String,
    /// `"incoming"` | `"outgoing"` | `"added"` | `"aprs"` | `"unknown"`.
    pub origin: String,
    /// Clamped to the operator's configured broadcast precision [R2-S9].
    pub grid: Option<String>,
    pub channels: Vec<PeerChannelDto>,
    // DROPPED on purpose: name/notes/email free text [R2-S11], and the
    // telnet endpoints wholesale — host:port is never agent-visible
    // (spec §AMENDMENT pt. 6).
}

/// Output of [`StationPort::find_peers`]: the curated peer roster.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PeerListDto {
    pub peers: Vec<PeerDto>,
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
    /// MUFday FRACTION per UTC hour (24-long): the fraction of days
    /// (`0.0..=1.0`) the dial frequency is below the predicted MUF at that
    /// hour. Not a frequency — the name carries the unit because a model and
    /// a judge both read the bare `mufday_by_hour` as MHz (surface-repair
    /// ledger row 7).
    pub mufday_fraction_by_hour: Vec<f64>,
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
    /// When this snapshot was last updated (unix ms). `None` when the values
    /// have NEVER been updated (`source: "shipped"`) — the previous shape
    /// stamped "now" on shipped data, making never-updated values look fresh
    /// to an agent told to judge freshness by this field (tuxlink-9n4cr).
    pub updated_at_ms: Option<u64>,
    /// Provenance of the data (e.g. `"shipped"`, `"noaa"`).
    pub source: String,
}

/// Result of an off-air WWV capture.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WwvCaptureDto {
    /// True when a confident decode updated the stored space-weather indices.
    pub updated: bool,
    /// True when audio was captured but no confident transcript was obtained.
    pub no_copy: bool,
    /// Provenance written to the snapshot, e.g. "rf-wwv-voice".
    pub source: String,
    pub sfi: Option<f64>,
    pub a_index: Option<f64>,
    pub k_index: Option<f64>,
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

/// UI spatial-help port (tuxlink-10bkw). `point_at` NEVER navigates, opens
/// panels, or fires actions — it asks the main webview to spotlight a
/// registered anchor and reports honestly whether that happened. Object-safe
/// so [`crate::McpState`] can hold it as `Arc<dyn UiHintPort>`.
#[async_trait]
pub trait UiHintPort: Send + Sync {
    /// `Ok(())` iff the hint is actually visible. `Err` carries the outcome:
    /// unknown-anchor (with the valid-ID list), anchor-unmounted (with the
    /// registry's "how to open this surface" line), overlay-busy, or timeout.
    async fn point_at(&self, anchor_id: &str) -> Result<(), PortError>;
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
    /// True iff the VARA setup engine ships in this build. Read-only.
    async fn vara_engine_available(&self) -> Result<bool, PortError>;
    /// Offline readiness probe (no network, no launch). Read-only.
    async fn vara_install_status(&self) -> Result<VaraInstallStatusDto, PortError>;
    /// Install VARA HF from a user-supplied installer `.exe` path. NON-TRANSMIT;
    /// gated only by pkexec's OS password prompt, not the transmit consent gate.
    async fn vara_install_start(
        &self,
        installer_path: String,
    ) -> Result<VaraInstallSummaryDto, PortError>;
    /// REDACTED content of the resolved instance's `VARA.ini` (registration
    /// code / password masked by construction). Read-only; local config, not
    /// wire content — no taint.
    async fn vara_ini_read(
        &self,
        prefix: Option<String>,
        instance: Option<String>,
    ) -> Result<String, PortError>;
    /// Classifier-weights readiness + job state (tuxlink-13ofm). Read-only,
    /// app-owned data — no taint.
    async fn classify_weights_status(&self) -> Result<ClassifyWeightsStatusDto, PortError>;
    /// Start (or retry) the classifier-weights download from this build's
    /// DEFAULT release source. NON-TRANSMIT local provisioning. Deliberately
    /// takes no URL: an agent-suppliable fetch target is an SSRF surface
    /// (Codex 2026-08-13 P1; pitfalls SSRF-1) — digest pinning protects what
    /// INSTALLS, not what the outbound GET can reach. Source overrides are an
    /// operator act, in the UI.
    async fn classify_weights_download(&self) -> Result<ClassifyWeightsStatusDto, PortError>;
    /// Cancel the running weights job. Partial files remain as the resume
    /// point; nothing partial can ever be mistaken for installed weights.
    async fn classify_weights_cancel(&self) -> Result<ClassifyWeightsStatusDto, PortError>;
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
    /// Read one documentation page in full, by the `slug` returned from `docs`.
    /// `Ok(None)` means the slug is unknown. App-owned content; does not taint.
    async fn doc(&self, slug: &str) -> Result<Option<DocBodyDto>, PortError>;
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
///
/// [`StationPort::find_peers`] is the DELIBERATE asymmetry on this trait: unlike
/// `find_stations` (public directory data, ungated), the peer roster is the
/// operator's PRIVATE station graph, so its impl gates the whole read behind the
/// egress arm [R2-S5]. Two methods on one trait with different gating postures is
/// intentional and required by spec — see the impl's asymmetry note.
#[async_trait]
pub trait StationPort: Send + Sync {
    /// Answer an intent-tagged station query with a BOUNDED, agent-native result
    /// (bd tuxlink-m0n38). Replaces the old raw-list `find_stations(filter) ->
    /// StationListDto` dump, which could emit the whole ~1,400-gateway catalog in
    /// one message and overflow the agent's context window. The result is bounded
    /// by construction (see [`crate::station_query`]); read-only, does not taint
    /// or gate.
    async fn find_stations(
        &self,
        request: crate::station_query::FindStationsRequest,
    ) -> Result<crate::station_query::FindStationsResponse, PortError>;
    /// List saved P2P peer stations. UNLIKE `find_stations`, this GATES the whole
    /// read behind the egress arm [R2-S5] (the roster is the operator's private
    /// social graph, not public directory data). Read-only; never transmits.
    async fn find_peers(&self) -> Result<PeerListDto, PortError>;
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

/// Off-air WWV space-weather capture. RECEIVE-ONLY: tunes the rig to WWV and
/// listens; never transmits. Yields parsed numeric indices, so nothing taints.
#[async_trait]
pub trait WwvPort: Send + Sync {
    /// Capture the next WWV bulletin off-air and ingest it.
    async fn capture(&self) -> Result<WwvCaptureDto, PortError>;
    /// Whether rig CAT control is configured (WWV capture needs it to tune).
    async fn cat_configured(&self) -> Result<bool, PortError>;
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
    /// The call was well-formed and authorized, but a required STATE
    /// precondition does not hold — session not open, devices not
    /// configured, modem busy. The message names the state (and usually the
    /// repairing call); the fix is a DIFFERENT call or a wait, never a
    /// rewrite of this one. Surfaced with this prefix so the reader stops
    /// misattributing precondition refusals to product-internal bugs
    /// (surface-repair ledger row 5 / zqo read P6).
    #[error("precondition not met (your call was fine; fix the named state, then retry): {0}")]
    Precondition(String),
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
    /// semantics as [`EgressPort::ardop_connect`]). `engine` selects which VARA
    /// engine the target uses (`None` → [`VaraEngineDto::VaraHf`], the
    /// backward-compatible default) — take it from the target peer channel's
    /// `transport` field, never guess.
    async fn vara_b2f_exchange(
        &self,
        target: String,
        intent: SessionIntentDto,
        freq_hz: Option<u64>,
        qsy_candidates: Option<Vec<QsyCandidateDto>>,
        engine: Option<VaraEngineDto>,
    ) -> Result<(), EgressPortError>;
    /// Open the VARA session: install the TCP transport to the local VARA
    /// engine and register MYCALL (the on-air station ID). PRE-AIR by itself
    /// (no RF leaves the radio), but it stands up a TRANSMIT-CAPABLE surface,
    /// so it runs in the same authority class as egress (mirrors the
    /// `rig_status` posture: an un-armed agent must not be able to open
    /// transmit-capable state). Required before
    /// [`EgressPort::vara_b2f_exchange`]; closed via the ungated
    /// [`AbortPort::vara_stop_session`]. `engine` selects which VARA engine to
    /// open (`None` → [`VaraEngineDto::VaraHf`], parity with the exchange
    /// tool's default) — take it from the target peer channel's `transport`
    /// field, never guess.
    async fn vara_open_session(
        &self,
        intent: SessionIntentDto,
        engine: Option<VaraEngineDto>,
    ) -> Result<(), EgressPortError>;
    /// Connect an AX.25 packet session to `call` over the optional digipeater
    /// `path`.
    async fn packet_connect(&self, call: String, path: Vec<String>) -> Result<(), EgressPortError>;
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
/// This enum's variants ARE the disposition vocabulary at the MCP write
/// boundary (mutation-contract epic; the routines boundary carries the same
/// vocabulary as `tuxlink_routines::error::Disposition`): each variant says
/// WHOSE DOING the refusal was, so the answer is data rather than something
/// reconstructed from prose after the fact.
pub enum WritePortError {
    /// The egress gate refused the Agent caller (unarmed / expired / tainted /
    /// poisoned). Carries the `EgressDenied` reason. Write tier only.
    /// Authority is absent; the fix is an operator act, not a better call.
    #[error("denied: {0}")]
    Denied(String),
    /// The agent-supplied input failed validation BEFORE the gate. The session's
    /// armed grant is not consumed. Attributable to the CALLER: retrying the
    /// same call yields the same refusal.
    #[error("invalid: {0}")]
    Invalid(String),
    /// The input is fine and the product cannot do it RIGHT NOW — backend
    /// offline, modem not running, radio busy. Environment and timing, not
    /// the caller: the same call can succeed later, and the Display text says
    /// so because the reader is an agent deciding what to do next.
    #[error("unavailable right now (not your call's fault; retry later): {0}")]
    Unavailable(String),
    /// The input was valid and the gate passed, but the operation itself
    /// failed for a cause not classified deeper (the seam returned a bare
    /// string, or it is genuinely our bug). Never the caller's doing.
    /// Classifying a site into [`Unavailable`](Self::Unavailable) when the
    /// cause is provably timing is the ongoing per-seam refinement.
    #[error("failed: {0}")]
    Failed(String),
}

impl WritePortError {
    /// Whether this refusal is a statement about the CALLER (their input
    /// violated the contract). Everything else must never be counted against
    /// the caller — the scoring predicate the bench's `is_model_attributable`
    /// established.
    pub fn is_caller_attributable(&self) -> bool {
        matches!(self, Self::Invalid(_))
    }

    /// Whether retrying the same call later can plausibly succeed with
    /// nobody changing anything.
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::Unavailable(_))
    }
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
    /// Stop-edit-start apply of assignments to VARA's own `VARA.ini`, with
    /// atomic write + timestamped backup, then relaunch + cmd-port verify
    /// (tuxlink-iww9r). Bounces the modem process.
    async fn vara_ini_apply(
        &self,
        dto: VaraIniApplyDto,
    ) -> Result<VaraIniApplyReportDto, WritePortError>;
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
    /// Save an attachment, returning the saved path.
    ///
    /// `dest` is OPTIONAL. When `None` the destination is DERIVED from the
    /// sanitized attachment filename, which makes every parameter of the call
    /// bounded and lets it proceed in a tainted session under the per-datum
    /// gate. When `Some`, it is caller-chosen free text and a tainted session
    /// is refused (tuxlink-0rc3h).
    async fn attachment_save(
        &self,
        folder: String,
        id: String,
        filename: String,
        dest: Option<String>,
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
    async fn catalog_send_inquiry(&self, item_ids: Vec<String>) -> Result<String, WritePortError>;
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

// ---------------------------------------------------------------------------
// FT-8 listener (tuxlink-dof5j) — receive-only. NOTHING here taints and
// NOTHING here is egress-gated.
//
// Taint: FT-8's payload is 77 bits over a fixed message-type set; `Standard`
// messages are packed callsign/grid/report FIELDS and free text is hard-capped
// at 13 characters of a restricted alphabet. A prompt injection does not fit in
// that channel, so tainting would block egress after listening — breaking the
// actual FT-8 loop (listen, then work the station you heard) to defend a threat
// the channel cannot carry. The threat model is calibrated to the CHANNEL's
// capacity, not the field's type.
//
// Gate: the listener never keys the transmitter. `set_band` DOES move the dial
// via CAT — a real-world side effect, but not a transmission, and in the same
// class as `rig_tune`'s dial move.
//
// The agent never sees the monolith's `Ft8Snapshot` (a UI struct: 40 slot
// records, health flags, sweep-dwell progress, device lists). It gets the
// purpose-shaped DTOs below.
// ---------------------------------------------------------------------------

/// One station heard on FT-8, aggregated across the decode ring.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Ft8HeardStationDto {
    pub call: String,
    pub grid: Option<String>,
    /// Best (highest) SNR seen for this station, in dB.
    pub best_snr_db: i32,
    /// Audio frequency of the most recent decode, in Hz.
    pub freq_hz: u32,
    pub band: String,
    pub last_heard_utc_ms: u64,
    /// How many times this station was decoded in the retained window.
    pub times_heard: u32,
}

/// Listener state, agent-shaped (NOT the UI's `Ft8Snapshot`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Ft8StatusDto {
    /// `"stopped"` | `"starting"` | `"listening"` | `"yielded"` | `"blocked"` |
    /// `"stopping"`.
    pub state: String,
    /// Present only when `state == "blocked"`; why it cannot listen.
    pub blocked_reason: Option<String>,
    pub band: String,
    pub dial_hz: u64,
    pub sweep_enabled: bool,
    pub device_name: Option<String>,
    pub last_slot_utc_ms: Option<u64>,
    pub last_failure: Option<String>,
}

/// One audio capture device the FT-8 listener can be pointed at. `stable_id` is
/// the value the operator/agent selects by.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Ft8AudioDeviceDto {
    pub human_name: String,
    pub stable_id: String,
}

/// FT-8 listener. Receive-only: nothing here keys the transmitter, so nothing
/// here is egress-gated. Decodes do not taint (see the module note above).
/// Object-safe so [`crate::McpState`] can hold it as `Arc<dyn Ft8Port>`.
#[async_trait]
pub trait Ft8Port: Send + Sync {
    /// Report the listener's state, band/dial, sweep, device, and what is
    /// blocking it. Read-only; does not taint or gate.
    async fn status(&self) -> Result<Ft8StatusDto, PortError>;
    /// The deduped stations heard in the retained decode window, most recently
    /// heard first. Read-only; does not taint or gate.
    async fn heard_stations(&self) -> Result<Vec<Ft8HeardStationDto>, PortError>;
    /// Start the listener (RECEIVE-ONLY; never transmits).
    async fn start(&self) -> Result<(), PortError>;
    /// Stop the listener and release the audio device.
    async fn stop(&self) -> Result<(), PortError>;
    /// Set the FT-8 band. QSYs the rig's dial via CAT when rig control is
    /// configured and the listener is running. Never transmits.
    async fn set_band(&self, band: &str) -> Result<(), PortError>;
    /// Enumerate the audio capture devices the listener can use.
    async fn list_audio_devices(&self) -> Result<Vec<Ft8AudioDeviceDto>, PortError>;
}

// ---------------------------------------------------------------------------
// Routines (spec §13) — operator-automation authoring + control. 10 tools,
// deliberately EXCLUDING consent-grant: the design-time transmit
// acknowledgment (spec §4) is recorded by a UI act only, and no method here
// takes a parameter that could supply it.
//
// `list`/`get`/`validate` are read-only. `save` never blocks on validation
// findings (spec §10: a half-written draft still saves). `enable`/`disable`
// convey a validation-blocked enable as `EnableResultDto { blocked: true,
// enabled: false }` — Ok, not Err (spec §10's "errors block enable" is a
// DTO field, not a tool failure); disable is never blocked. `run` is the one
// method with its own error type ([`RoutinesRunError`]): a blocked run OR an
// `automatic`-transmit routine's missing design-time acknowledgment surfaces
// as [`RoutinesRunError::Refused`], carrying the SAME message the commands
// layer produces, verbatim — the router does not add remedy text (unlike
// [`EgressPortError::Denied`]/[`WritePortError::Denied`], this is never an
// `EgressGuard` arm/taint decision). `dry_run` is refused by NOTHING: it
// routes through the engine's fake-world entry point, which swaps every
// action for a capability-mirroring fake BEFORE the executor resolves one —
// structurally unable to touch a real action, whatever the routine's
// validation/consent state.
//
// None of these methods taint the session or pass through the `EgressGuard`:
// routines authoring/control is local file + engine state, not egress.
// ---------------------------------------------------------------------------

/// Validation severity, mirroring the engine's `Severity` (spec §10: `Error`
/// blocks enable/run; `Warning` never does).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum FindingSeverityDto {
    Error,
    Warning,
}

/// One validation finding, mirroring the engine's `Finding` field-for-field.
/// `message` is the operator-facing explanation and ALWAYS names the
/// offending entity verbatim (spec §10) — nothing in this crate paraphrases
/// it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FindingDto {
    /// Machine-readable class, e.g. `"UNRESOLVED_REF"`, `"UNKNOWN_ACTION"`.
    pub code: String,
    pub severity: FindingSeverityDto,
    pub routine: String,
    /// The track this finding is scoped to, when it is track-scoped.
    pub track: Option<String>,
    /// The step this finding is scoped to, when it is step-scoped.
    pub step: Option<String>,
    pub message: String,
}

/// The agent-facing outcome class of a routine authoring op (save/edit/validate).
/// The load-bearing field of the disposition: it tells a weak model whether to
/// repair the routine itself, or to stop because only the operator can proceed
/// (tuxlink-kbh4t).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum DispositionState {
    /// No blocking findings; enable/run-ready.
    Valid,
    /// Blocked, but an agent-only edit clears it (see `remedies`).
    InvalidAgentRepairable,
    /// Saved, but preserving the requested behavior needs an OPERATOR
    /// acknowledgment (design-time). Paired with `agent_terminal: true` — the
    /// agent must stop and tell the user, not loop or coerce.
    SavedNeedsOperator,
}

/// Who can apply a remedy.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum RemedyActor {
    Agent,
    Operator,
}

/// A concrete step toward a valid routine. An `agent` remedy names the exact
/// tool + arguments to apply (revision-bound); an `operator` remedy names NO
/// tool — acknowledgment is operator authority and never an agent op.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RemedyDto {
    pub actor: RemedyActor,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub routine: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patch: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_revision: Option<String>,
    pub changes_behavior: bool,
    pub consequence: String,
}

impl RemedyDto {
    /// The operator records the acknowledgment in the designer — no agent path.
    pub fn operator_acknowledge(routine: &str) -> Self {
        Self {
            actor: RemedyActor::Operator,
            tool: None,
            routine: Some(routine.into()),
            patch: None,
            expected_revision: None,
            changes_behavior: false,
            consequence: "the operator records the acknowledgment in the routine designer; \
                          it cannot be granted over MCP"
                .into(),
        }
    }

    /// Switch `routine` to attended via `routines_meta_set`. Bound to `revision`
    /// when known (empty string => omitted, agent supplies the current revision).
    pub fn set_attended(routine: &str, revision: &str) -> Self {
        Self {
            actor: RemedyActor::Agent,
            tool: Some("routines_meta_set".into()),
            routine: Some(routine.into()),
            patch: Some(serde_json::json!({ "transmit_mode": "attended" })),
            expected_revision: (!revision.is_empty()).then(|| revision.to_string()),
            changes_behavior: true,
            consequence: "scheduled runs park at each transmission until a person confirms".into(),
        }
    }
}

/// The typed authoring disposition attached to a routine save/edit/validate
/// result. Computed at the port layer from the findings + the routine's mode;
/// the validator crate never names MCP tools.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AuthoringDispositionDto {
    pub state: DispositionState,
    pub agent_terminal: bool,
    pub remedies: Vec<RemedyDto>,
    /// The finding codes that actually produced a non-`Valid` state, deduped
    /// in first-seen order. Empty exactly when `state` is `Valid`.
    ///
    /// tuxlink-lnctz: severity is already per-finding, but a model reading a
    /// seven-finding array plus a bare `blocked` flag has to cross-reference
    /// them to learn WHICH one blocks — and in Ladder-2 `base/S4/rev_off` it
    /// did not. That routine was held non-`Valid` for all 39 mutating calls
    /// by one `UNRESOLVED_REF` on an unconfigurable preset, while the model
    /// rewrote control flow it had no reason to touch. Naming the blocker
    /// costs nothing and fabricates no remedy.
    pub blocked_by: Vec<String>,
    /// ENVIRONMENTAL warning codes present that do NOT block, deduped in
    /// first-seen order. Environmental means editing the routine cannot clear
    /// them (no rig configured, no internet, station-profile facts).
    ///
    /// Withholding a remedy from a warning was meant to signal "acceptable,
    /// stop" (see [`Self::classify`]). Ladder-2 shows silence does not read
    /// that way: models fill it with a repair loop invented from the message
    /// prose. Listing the warnings as acceptable states it positively, which
    /// silence never did, without offering an edit to apply.
    pub acceptable_warnings: Vec<String>,
    /// REPAIRABLE structural warning codes, deduped in first-seen order — the
    /// [`ADVISORY_CODES`] subset of the warnings present. Split out of
    /// `acceptable_warnings` (tuxlink-0hjm4) because the completion sentence
    /// declares everything in that list "environmental and cannot be repaired
    /// by editing this routine", which is FALSE for structural warnings and
    /// coaches ignoring them: lift1-base E2 left a dead `data.spacewx_swpc`
    /// read in place with `OUTPUT_NEVER_CONSUMED` on the wire, and the saved
    /// routine silently dropped its propagation gate. Advisories do not block
    /// save or enable; each finding's message says what to change.
    #[serde(default)]
    pub advisories: Vec<String>,
    /// Plain-prose completion statement, present EXACTLY when `state` is
    /// `Valid`. Listing `acceptable_warnings` positively (above) was meant to
    /// stop warning-driven repair loops; the Laguna P1 probe (2026-07-28, 37
    /// consecutive polish edits against a green routine) showed a model whose
    /// learned stop criterion is "validation returns clean" still reads any
    /// warning as not-done. This sentence states the stop signal explicitly
    /// on the wire, where the loop actually happens. Absent (not null) for
    /// every non-`Valid` state per the null-noise discipline.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completion: Option<String>,
}

/// The completion sentence for `Valid` dispositions — see the field docs.
fn valid_completion() -> Option<String> {
    Some(
        "The routine is COMPLETE. Any codes in acceptable_warnings are \
         environmental and cannot be repaired by editing this routine; do \
         not make further edits for them. Report completion to the user."
            .to_string(),
    )
}

/// The completion sentence for `Valid` dispositions that still carry
/// advisories: the routine saves and enables, but declaring it COMPLETE
/// would coach ignoring repairable structural defects (the lift1-base E2
/// evidence on [`AuthoringDispositionDto::advisories`]). ASCII-only on
/// purpose (operator, 2026-07-29): this copy crosses into arbitrary model
/// harnesses, and a non-UTF8-clean hop turns an em-dash into mojibake
/// mid-instruction.
fn advisory_completion() -> Option<String> {
    Some(
        "The routine is saved and can be enabled, but the codes in \
         advisories are repairable defects in this routine's structure; each \
         finding's message says what to change. Repair them with the edit \
         verbs, or tell the user why the flagged shape is intentional. Codes \
         in acceptable_warnings are environmental and cannot be repaired by \
         editing this routine; do not make further edits for those."
            .to_string(),
    )
}

/// The warning codes that are REPAIRABLE by editing the routine — the
/// `advisories` split of [`AuthoringDispositionDto::classify`]. Everything
/// else stays in `acceptable_warnings` (environmental). String literals, not
/// imports: mcp-core deliberately does not depend on the validator crate;
/// the names are pinned by the fixture corpus and the strings-gate.
const ADVISORY_CODES: &[&str] = &[
    // structure.rs
    "NO_TERMINAL_PATH",
    "ARM_FALLTHROUGH_LEAK",
    "BRANCH_BOTH_ARMS_EMPTY",
    "TX_ONLY_ON_FAILURE_ARM",
    "ARM_END_INVERTED",
    "REPEAT_CONNECT_NO_DELAY",
    // contracts.rs
    "OUTPUT_NEVER_CONSUMED",
    // capability.rs (the outbox and timeout shapes are authored, not
    // station-profile facts)
    "COMPOSE_AFTER_CONNECT",
    "CONNECT_NOTHING_STAGED",
    "STEP_TIMEOUT_LIKELY_INSUFFICIENT",
    // triggers.rs: the finding's own message instructs a definition edit
    // (split the cadences into separate routines) — Codex 2026-07-29 P2
    "MULTIPLE_SCHEDULES",
    // params.rs (surface-repair row 9): a param the action does not declare
    // is silently ignored at runtime — the step false-succeeds with part of
    // its payload dropped. That is a REPAIRABLE authoring defect (rename or
    // remove the param), not an environmental fact; leaving it in
    // acceptable_warnings had the completion prose calling it "cannot be
    // repaired by editing", which coached ignoring it (zqo
    // AS-CATALOG-ROUNDTRIP a1: an entire compose payload silently unused).
    "UNKNOWN_PARAM",
];

/// Dedupe finding codes preserving first-seen order — a routine commonly
/// carries the same code on several steps (four `NO_RIG_CONFIGURED` in the
/// S4 trace) and repeating it adds noise, not information.
fn dedup_codes(codes: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    codes.into_iter().filter(|c| seen.insert(c.clone())).collect()
}

impl AuthoringDispositionDto {
    /// Classify a routine authoring result from its findings alone. The consent
    /// error codes already encode the mode (the `AUTO_*_UNACKED` codes fire only
    /// for `automatic` routines), so no separate mode input is needed. Computed
    /// at the port layer — the validator crate never names MCP tools.
    pub fn classify(findings: &[FindingDto], routine: &str, revision: &str) -> Self {
        let blocking: Vec<&FindingDto> = findings
            .iter()
            .filter(|f| f.severity == FindingSeverityDto::Error)
            .collect();
        let acceptable_warnings = dedup_codes(
            findings
                .iter()
                .filter(|f| {
                    f.severity == FindingSeverityDto::Warning
                        && !ADVISORY_CODES.contains(&f.code.as_str())
                })
                .map(|f| f.code.clone()),
        );
        let advisories = dedup_codes(
            findings
                .iter()
                .filter(|f| {
                    f.severity == FindingSeverityDto::Warning
                        && ADVISORY_CODES.contains(&f.code.as_str())
                })
                .map(|f| f.code.clone()),
        );
        let blocked_by = dedup_codes(blocking.iter().map(|f| f.code.clone()));
        if blocking.is_empty() {
            // Warnings (e.g. ATTENDED_UNDER_SCHEDULE) are an ACCEPTABLE terminal
            // state, never a remedy to apply. Withholding the remedy was meant
            // to stop the ping-pong on its own; tuxlink-lnctz shows it does not
            // (Ladder-2 ran a 34-turn warning-driven loop with no remedy ever
            // offered), so `acceptable_warnings` now says so positively instead
            // of leaving the model to infer it from an empty array.
            // Advisories keep the state Valid (they never block save or
            // enable) but swap the completion sentence: "COMPLETE, stop"
            // over a repairable structural defect is exactly the coaching
            // that let lift1-base E2 ship a dead read (tuxlink-0hjm4).
            let completion = if advisories.is_empty() {
                valid_completion()
            } else {
                advisory_completion()
            };
            return Self {
                state: DispositionState::Valid,
                agent_terminal: false,
                remedies: vec![],
                blocked_by,
                acceptable_warnings,
                advisories,
                completion,
            };
        }
        // The routine's OWN automatic-unattended transmit/write: only the operator
        // can acknowledge unattended TX (never an agent op), so the agent stops.
        // Attended is offered as a behavior-changing alternative, not "the fix".
        if blocking
            .iter()
            .any(|f| f.code == "AUTO_TX_UNACKED" || f.code == "AUTO_WRITE_UNACKED")
        {
            return Self {
                state: DispositionState::SavedNeedsOperator,
                agent_terminal: true,
                remedies: vec![
                    RemedyDto::operator_acknowledge(routine),
                    RemedyDto::set_attended(routine, revision),
                ],
                blocked_by,
                acceptable_warnings,
                advisories,
                completion: None,
            };
        }
        // A callee the runtime child-start gate would refuse: the honest agent
        // fix is making THAT callee attended (its revision is unknown here — the
        // agent supplies the current one).
        if let Some(f) = blocking.iter().find(|f| f.code == "CALLEE_CONSENT_UNREACHABLE") {
            let callee = callee_name_from_message(&f.message).unwrap_or_default();
            return Self {
                state: DispositionState::InvalidAgentRepairable,
                agent_terminal: false,
                remedies: vec![RemedyDto::set_attended(&callee, "")],
                blocked_by,
                acceptable_warnings,
                advisories,
                completion: None,
            };
        }
        // Any other blocking finding with no known agent-only edit: an honest
        // stop, no fabricated remedy. `blocked_by` still names what is holding
        // it, which is the difference between an honest stop and an opaque one.
        Self {
            state: DispositionState::SavedNeedsOperator,
            agent_terminal: true,
            remedies: vec![],
            blocked_by,
            acceptable_warnings,
            advisories,
            completion: None,
        }
    }
}

/// Extract the callee name from a `CALLEE_CONSENT_UNREACHABLE` message of the
/// form `... calls "NAME", which ...`. Best-effort; `None` if not parseable.
fn callee_name_from_message(msg: &str) -> Option<String> {
    msg.split("calls \"")
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .map(String::from)
}

/// [`RoutinesPort::validate`]'s result: the findings plus the typed disposition.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ValidateResultDto {
    pub findings: Vec<FindingDto>,
    pub disposition: AuthoringDispositionDto,
}

/// One routine library entry ([`RoutinesPort::list`]). `trigger_kinds` is
/// curated down to each trigger's tag (`"schedule"` / `"manual"`) — mcp-core
/// stays free of the routines engine's full trigger/step/track type graph;
/// the complete definition is available verbatim via [`RoutinesPort::get`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RoutineSummaryDto {
    pub routine: String,
    /// `"attended"` or `"automatic"`.
    pub transmit_mode: String,
    pub enabled: bool,
    pub trigger_kinds: Vec<String>,
}

/// [`RoutinesPort::save`]'s result. The routine IS saved regardless of
/// `findings` (spec §10: save never blocks) — `blocked` is the pre-computed
/// "cannot enable/run as it stands" bit.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SaveResultDto {
    pub routine: String,
    /// Revision token of the stored definition (D7 CAS): pass it back as
    /// `expected_revision` on a later save/edit to detect a lost update.
    #[serde(default)]
    pub revision: String,
    pub findings: Vec<FindingDto>,
    pub blocked: bool,
    /// Typed, machine-actionable outcome for the agent (tuxlink-kbh4t).
    pub disposition: AuthoringDispositionDto,
}

/// [`RoutinesPort::get`]'s result: the definition (the exact shape
/// `routines_save`'s `def` accepts) plus its revision token.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RoutineGetDto {
    pub revision: String,
    pub def: serde_json::Value,
}

/// [`RoutinesPort::save`]'s request. EXACTLY ONE of `def` (a JSON object —
/// the preferred form) or `def_json` (deprecated string form) must be
/// present: both or neither is invalid input. A string supplied as `def`
/// that parses as one JSON OBJECT is tolerated and parsed (tuxlink-8fcbh,
/// amending adrev A7's never-auto-parse rule; see [`crate::arg_shape`] for
/// the boundary-wide rule tuxlink-sq72z extended to every verb tool); any
/// other string in `def` stays invalid input.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct SaveRoutineRequestDto {
    #[serde(default)]
    pub def: Option<serde_json::Value>,
    #[serde(default)]
    pub def_json: Option<String>,
    #[serde(default)]
    pub expected_revision: Option<String>,
}

/// One branch-arm entry an edit repaired (the designer-parity scrub).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScrubbedRefDto {
    pub branch: String,
    pub arm: String,
    pub step: String,
}

/// Every edit verb's result (spec D6 outcome 3): the edit is APPLIED AND
/// SAVED — outcomes 1/2 (malformed input, precondition failure) are port
/// errors carrying a `[CODE]`-prefixed message and mutate nothing.
/// `step_findings` are the validator findings anchored to the touched step;
/// everything else is `routine_findings`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EditResultDto {
    pub routine: String,
    pub revision: String,
    pub applied: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scrubbed: Vec<ScrubbedRefDto>,
    pub step_findings: Vec<FindingDto>,
    pub routine_findings: Vec<FindingDto>,
    pub blocked: bool,
    /// Typed, machine-actionable outcome for the agent (tuxlink-kbh4t).
    pub disposition: AuthoringDispositionDto,
}

/// [`RoutinesPort::rename`]'s result.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RenameResultDto {
    pub routine: String,
    pub revision: String,
    pub enabled: bool,
    pub callers_updated: Vec<String>,
}

/// One fragment edit (spec D1). The router exposes each variant as its own
/// flat MCP tool — small models handle flat per-verb schemas better than one
/// nested op array — and builds this DTO; the port trait carries ONE `edit`
/// method so implementors and mocks stay compact. Placement fields on
/// `StepAdd`/`StepMove`: give exactly one of `track` (append), `after_step_id`
/// (splice after), or `branch_step_id`+`branch_arm` (into a branch arm,
/// optionally positioned by `branch_after_step_id`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum RoutineEditOpDto {
    StepAdd {
        step: serde_json::Value,
        #[serde(default)]
        track: Option<String>,
        #[serde(default)]
        after_step_id: Option<String>,
        #[serde(default)]
        branch_step_id: Option<String>,
        #[serde(default)]
        branch_arm: Option<String>,
        #[serde(default)]
        branch_after_step_id: Option<String>,
    },
    StepUpdate {
        step_id: String,
        patch: serde_json::Value,
    },
    StepRemove {
        step_id: String,
    },
    StepMove {
        step_id: String,
        #[serde(default)]
        track: Option<String>,
        #[serde(default)]
        after_step_id: Option<String>,
        #[serde(default)]
        branch_step_id: Option<String>,
        #[serde(default)]
        branch_arm: Option<String>,
        #[serde(default)]
        branch_after_step_id: Option<String>,
    },
    TrackAdd {
        track: String,
    },
    TrackRemove {
        track: String,
    },
    TriggerSet {
        triggers: serde_json::Value,
    },
    MetaSet {
        patch: serde_json::Value,
    },
}

/// [`RoutinesPort::edit`]'s request.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RoutineEditRequestDto {
    pub routine: String,
    #[serde(default)]
    pub expected_revision: Option<String>,
    pub op: RoutineEditOpDto,
}

/// [`RoutinesPort::enable`]/[`RoutinesPort::disable`]'s result. `enabled` is
/// the state the routine is ACTUALLY in after the call: a refused enable
/// reports `enabled: false, blocked: true` plus the blocking findings — this
/// is how spec §10's "errors block enable" reaches the agent (a DTO field,
/// never a tool error).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EnableResultDto {
    pub routine: String,
    pub enabled: bool,
    pub blocked: bool,
    pub findings: Vec<FindingDto>,
}

/// A run's state (spec §8), mirroring the engine's `RunState` exactly.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunStateDto {
    Pending,
    Running,
    Waiting,
    AwaitingConsent,
    AwaitingRadio,
    Completed,
    Failed,
    Cancelled,
    Interrupted,
}

/// Fast in-memory answer to [`RoutinesPort::run_status`]. The full,
/// step-by-step record is [`RoutinesPort::journal_get`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RunStatusDto {
    pub run_id: String,
    pub routine: String,
    pub dry_run: bool,
    pub state: RunStateDto,
}

/// [`RoutinesPort::dry_run`]'s start response: the run id to poll, plus the
/// validator's findings (informational only — a dry run is never blocked by
/// them; see the trait doc).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DryRunStartedDto {
    pub run_id: String,
    pub findings: Vec<FindingDto>,
}

/// Failure modes [`RoutinesPort::run`] can surface. Distinct from
/// [`EgressPortError`]/[`WritePortError`]: a routines refusal is NEVER an
/// `EgressGuard` arm/taint decision — routines authoring/running has nothing
/// to do with that gate — so the router must NOT append the "ask the
/// operator to ARM" remedy text those two attach to their `Denied` variant.
/// [`RoutinesRunError::Refused`] is surfaced to the agent completely
/// verbatim.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum RoutinesRunError {
    /// The routine name is unknown.
    #[error("routine not found")]
    NotFound,
    /// The run was refused: a blocking validation error (spec §10), or an
    /// `automatic`-transmit routine missing its design-time acknowledgment
    /// (spec §4/§13, recorded by a UI act only — no method on this trait can
    /// supply it). The string IS the operator-facing refusal text, exactly as
    /// the commands layer produced it; the router does not rewrite it.
    #[error("{0}")]
    Refused(String),
    /// An operational failure after the checks above passed.
    #[error("internal error: {0}")]
    Internal(String),
}

/// One declared parameter of an action (tuxlink-3nvvl): the machine-readable
/// param contract the validator lints against — `type` is the registry's
/// snake_case value-type token (`"string"`, `"number"`, `"boolean"`,
/// `"string_list"`, `"band_list"`, `"station_list"`, `"object_list"`,
/// `"object"`). List-typed params accept a whole-value step ref
/// (`"$sN.key"`) when the referenced output is list-typed; `["$sN.key"]`
/// where `key` is a list fails validation (array-of-arrays at runtime).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ParamSpecDto {
    pub key: String,
    #[serde(rename = "type")]
    pub value_type: String,
    pub required: bool,
    pub description: String,
    /// Closed vocabulary for string params / list elements, when declared.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed: Option<Vec<String>>,
    /// Paste-ready example for THIS param alone, as a real JSON value.
    pub example: serde_json::Value,
}

/// One declared output of an action (tuxlink-3nvvl): what `$sN.<key>`
/// resolves to. Same `type` token vocabulary as [`ParamSpecDto`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OutputSpecDto {
    pub key: String,
    #[serde(rename = "type")]
    pub value_type: String,
    pub description: String,
    /// May be null or absent depending on the run's path — branch before
    /// feeding it to a required param.
    #[serde(default)]
    pub nullable: bool,
}

/// One authorable routine action, curated for an AGENT author (tuxlink-dngvs).
/// The fields mirror the engine's `ActionDescriptor` (the same registry the
/// designer palette renders — ADR 0024: one capability tree), minus the
/// UI/engine-only bits (`dry_run_shape`). `example_params` is the exact
/// compact-JSON string the palette seeds — the canonical "what do this
/// action's params look like" answer.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ActionInfoDto {
    pub name: String,
    pub label: String,
    pub description: String,
    pub needs_radio: bool,
    /// Consent class: an attended run parks a transmitting step for operator
    /// confirmation before it runs.
    pub transmits: bool,
    /// Consent class: mutates persisted station configuration; parks like a
    /// transmit for operator confirmation in attended runs.
    pub writes_config: bool,
    pub needs_internet: bool,
    /// Canonical example `params` as a JSON OBJECT — paste-ready into an
    /// `ActionStep.params` field. Deliberately NOT the registry's compact
    /// string form: a string-in-JSON example invites the author to paste a
    /// string where an object belongs, recreating the def_json
    /// double-encoding trap this catalog exists to end (Codex adrev
    /// 2026-07-19 P2 #1). `None` when the action takes no params.
    pub example_params: Option<serde_json::Value>,
    /// A closed vocabulary for ONE string param: `(param_key, allowed…)` —
    /// a literal value outside the set fails validation.
    pub allowed_values: Option<(String, Vec<String>)>,
    /// Declared per-param contracts (tuxlink-3nvvl). Empty when the action
    /// has not declared its param surface (param validation then skips it).
    #[serde(default)]
    pub params: Vec<ParamSpecDto>,
    /// Declared step outputs — the `$sN.<key>` reference surface.
    #[serde(default)]
    pub outputs: Vec<OutputSpecDto>,
}

/// One trigger kind a routine's `triggers` array accepts (tuxlink-dngvs).
/// `fields` documents the kind's parameters field-by-field; `example` is a
/// paste-ready triggers entry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TriggerKindDto {
    pub r#type: String,
    pub description: String,
    pub fields: serde_json::Value,
    pub example: serde_json::Value,
}

/// One control-flow step kind (`branch` / `delay` / `retry` / `call` /
/// `end`), documented for an agent author (tuxlink-6epl8): battery S1 ran
/// four model families against `Control::Branch` and NONE guessed its flat
/// `on`/`op`/`value` + then/else-id-list shape - the catalog taught actions
/// and triggers but left every control shape to invention. `fields`
/// documents the kind field-by-field; `example` is a paste-ready step
/// object (for `branch`, the strict-boolean form; `comparison_example`
/// carries the op/value form and is absent on every other kind).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ControlInfoDto {
    pub control: String,
    pub description: String,
    pub fields: serde_json::Value,
    pub example: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comparison_example: Option<serde_json::Value>,
}

/// [`RoutinesPort::actions_catalog`]'s result: everything an author needs to
/// write a valid routine WITHOUT guessing — the action set with params and
/// consent classes, the control-flow step kinds with their exact shapes,
/// the trigger kinds, and one complete example definition. Built for the
/// agent path (the human path is the designer palette over the same
/// registry).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ActionsCatalogDto {
    pub actions: Vec<ActionInfoDto>,
    /// Every control-flow step kind with its exact wire shape
    /// (tuxlink-6epl8). `#[serde(default)]` keeps older serialized payloads
    /// deserializable.
    #[serde(default)]
    pub controls: Vec<ControlInfoDto>,
    pub trigger_kinds: Vec<TriggerKindDto>,
    /// One COMPLETE, minimal, valid routine definition — the exact JSON shape
    /// `routines_save` accepts (tuxlink-rt4ey). This teaches the ENVELOPE
    /// (`routine` is the routine's NAME string; `schema_version`;
    /// `transmit_mode`; `triggers` is a LIST; steps live under
    /// `tracks[].steps` with an `end` control), which the action/trigger
    /// entries alone cannot: a live model mirrored this DTO's own response
    /// shape as the definition schema and looped 14 saves on envelope parse
    /// errors. Copy it, then substitute actions/params/triggers from the
    /// entries above.
    pub definition_template: serde_json::Value,
}

/// Routines: the 11-tool MCP surface for the operator-automation feature
/// (spec §13) — `list` / `get` / `validate` / `save` / `enable` / `disable` /
/// `run` / `run_status` / `journal_get` / `dry_run` / `actions_list`. See the
/// module note above for the shared error/blocking conventions. Object-safe so
/// [`crate::McpState`] can hold it as `Arc<dyn RoutinesPort>`.
#[async_trait]
pub trait RoutinesPort: Send + Sync {
    /// List every routine in the library. Read-only.
    async fn list(&self) -> Result<Vec<RoutineSummaryDto>, PortError>;
    /// The authoring catalog: every registered action (name, description,
    /// params example, consent class) + every trigger kind, so an agent can
    /// author a routine without inventing names (tuxlink-dngvs — two live
    /// models guessed `modem.vara.*` names and were rejected one
    /// `UNKNOWN_ACTION` at a time). Read-only.
    async fn actions_catalog(&self) -> Result<ActionsCatalogDto, PortError>;
    /// Read one routine's full definition, exactly as stored (spec §14 JSON
    /// shape — the same shape [`RoutinesPort::save`]'s `def` accepts), plus
    /// its revision token for the D7 lost-update check. `Err`
    /// ([`PortError::NotFound`]) when the name is unknown. Read-only.
    async fn get(&self, name: &str) -> Result<RoutineGetDto, PortError>;
    /// Validate one routine by name against the live station, WITHOUT saving
    /// or running anything — the SAME validator [`RoutinesPort::save`] /
    /// [`RoutinesPort::run`] use (spec §10: one validator, no privileged
    /// path). Read-only.
    async fn validate(&self, name: &str) -> Result<ValidateResultDto, PortError>;
    /// Save a routine definition (spec §14 shape) from EXACTLY ONE of
    /// `req.def` (object, preferred) or `req.def_json` (deprecated string).
    /// NEVER refused by validation findings — a half-written draft still
    /// saves; `findings`/`blocked` in the result say what is wrong. Refused
    /// on a parse failure, a routine name that would escape the routine
    /// store, a violated exactly-one rule, or a stale `expected_revision`
    /// (`REVISION_CONFLICT` — the D7 lost-update check).
    async fn save(&self, req: SaveRoutineRequestDto) -> Result<SaveResultDto, PortError>;
    /// Apply ONE fragment edit (spec D1/D6) to a SAVED, currently-DISABLED
    /// routine: the nine `routines_step_*`/`routines_track_*`/
    /// `routines_trigger_set`/`routines_meta_set` tools all funnel here. An
    /// enabled routine is refused (`ROUTINE_ENABLED`, the D5 guard); a stale
    /// `expected_revision` is refused (`REVISION_CONFLICT`); malformed
    /// payloads are refused with a `[CODE]`-prefixed teaching message. All
    /// refusals mutate NOTHING. An applied edit is saved even with error
    /// findings (errors block enable/run, never save).
    async fn edit(&self, req: RoutineEditRequestDto) -> Result<EditResultDto, PortError>;
    /// Transactional rename (spec D1, adrev A5): definition file, body name,
    /// enabled state, and `call` references in other routines migrate in one
    /// operation. Works on an enabled routine (content unchanged — no
    /// re-gate); refused when the new name is taken or invalid.
    async fn rename(
        &self,
        routine: &str,
        new_name: &str,
        expected_revision: Option<String>,
    ) -> Result<RenameResultDto, PortError>;
    /// Enable a routine so its triggers can fire it. See the module note for
    /// the Ok-with-`blocked`-flag contract; `Err` only for an unknown name.
    async fn enable(&self, name: &str) -> Result<EnableResultDto, PortError>;
    /// Disable a routine. Never blocked, however invalid the routine
    /// currently is; `Err` only for an unknown name.
    async fn disable(&self, name: &str) -> Result<EnableResultDto, PortError>;
    /// Start a real run with the given JSON-object `args_json`. Refused
    /// ([`RoutinesRunError::Refused`], verbatim) when a validation error
    /// blocks it, or when the routine is `automatic`-transmit and lacks its
    /// design-time acknowledgment (spec §4/§13). An automatic routine that
    /// ALREADY carries the acknowledgment runs the SAME whether invoked from
    /// the UI or from here — the acknowledgment is a design-time gate that
    /// covers every invoker, not a per-caller consent (this is deliberate,
    /// not a gap). Returns the run id.
    async fn run(&self, name: &str, args_json: String) -> Result<String, RoutinesRunError>;
    /// Start a DRY run — the fake world (spec §10 layer 3). Refused by
    /// NOTHING: not a blocking validation error, not a missing automatic-
    /// transmit acknowledgment — rehearsing an as-yet-unfit-to-run routine is
    /// the point. The impl MUST route this through the engine's dedicated
    /// fake-world entry point (mirroring
    /// [`RoutinesState::start_dry_run`](../../../src/routines/session.rs)),
    /// which swaps every action for a capability-mirroring fake BEFORE the
    /// executor resolves one, so this is structurally unable to touch a real
    /// action — no rig seized, no carrier keyed, no message queued, no
    /// gateway dialed, regardless of `script_json`. `args_json` is the JSON
    /// input object; `script_json`, when present, is a JSON object shaping
    /// the fake world's per-action outcomes (absent = an all-succeeds fake
    /// world).
    async fn dry_run(
        &self,
        name: &str,
        args_json: String,
        script_json: Option<String>,
    ) -> Result<DryRunStartedDto, PortError>;
    /// Fast in-memory run status. `Ok(None)` when the run id is unknown.
    /// Read-only.
    async fn run_status(&self, run_id: &str) -> Result<Option<RunStatusDto>, PortError>;
    /// The full, durable step-by-step journal for a run, each entry VERBATIM
    /// (spec §11) — a failed step's cause is the actual VARA/CAT/HTTP failure
    /// text the action surfaced, never paraphrased. `Err`
    /// ([`PortError::NotFound`]) for an unknown run id. Read-only.
    async fn journal_get(&self, run_id: &str) -> Result<Vec<serde_json::Value>, PortError>;
}

#[cfg(test)]
mod authoring_disposition_tests {
    use super::*;

    fn err(code: &str, msg: &str) -> FindingDto {
        FindingDto {
            code: code.into(),
            severity: FindingSeverityDto::Error,
            routine: "r".into(),
            track: None,
            step: None,
            message: msg.into(),
        }
    }

    fn warn(code: &str, msg: &str) -> FindingDto {
        FindingDto {
            code: code.into(),
            severity: FindingSeverityDto::Warning,
            routine: "r".into(),
            track: None,
            step: None,
            message: msg.into(),
        }
    }

    /// Ledger row 6 (CLOSED): unset packet-config fields are explicit nulls
    /// on the wire, never `0`/`""` sentinels — a model reads a sentinel as a
    /// configured value. The keys stay PRESENT (null, not absent) so the
    /// field inventory of a config read is stable across configured and
    /// unconfigured stations.
    #[test]
    fn packet_config_unset_fields_serialize_as_null_not_sentinels() {
        let unset = PacketConfigDto {
            kiss_host: None,
            kiss_port: None,
            baud: None,
            tx_delay: 30,
        };
        let j = serde_json::to_value(&unset).unwrap();
        assert!(j["kiss_host"].is_null(), "unset host must be null: {j}");
        assert!(j["kiss_port"].is_null(), "unset port must be null: {j}");
        assert!(j["baud"].is_null(), "unset baud must be null: {j}");
        assert_eq!(j["tx_delay"], 30);

        let configured = PacketConfigDto {
            kiss_host: Some("127.0.0.1".into()),
            kiss_port: Some(8001),
            baud: Some(1200),
            tx_delay: 30,
        };
        let j = serde_json::to_value(&configured).unwrap();
        assert_eq!(j["kiss_host"], "127.0.0.1");
        assert_eq!(j["kiss_port"], 8001);
        assert_eq!(j["baud"], 1200);
    }

    /// Ledger row 2 (tuxlink-wovan): the modem-status wire carries its own
    /// memory and its own caveats — `last_session` discloses what just
    /// happened after the transient session returns the surface to idle, and
    /// `selected.note` states on the wire that `selected` is the persisted
    /// target, not a live link (a model read the sticky value as "active").
    #[test]
    fn modem_status_wire_carries_last_session_and_selected_note() {
        let dto = ModemStatusDto {
            kind: "idle".into(),
            connected: false,
            state: "idle".into(),
            running: vec![],
            selected: Some(SelectedConnectionDto {
                session_type: "radio".into(),
                protocol: "vara-hf".into(),
                note: SELECTED_CONNECTION_NOTE.to_string(),
            }),
            conflict: false,
            last_session: Some(LastSessionSummaryDto {
                transport: "ardop".into(),
                target: Some("W6XYZ".into()),
                outcome: "completed".into(),
                detail: None,
                ended_at_ms: 1_755_400_000_000,
            }),
        };
        let j = serde_json::to_value(&dto).unwrap();
        assert_eq!(j["last_session"]["transport"], "ardop");
        assert_eq!(j["last_session"]["outcome"], "completed");
        assert_eq!(j["last_session"]["target"], "W6XYZ");
        assert!(j["last_session"]["detail"].is_null());
        // The note must actually teach: name the live-vs-persisted split and
        // point at `running`.
        let note = j["selected"]["note"].as_str().unwrap();
        assert!(note.contains("NOT what is live"), "note teaches: {note}");
        assert!(note.contains("running"), "note points at running: {note}");
    }

    #[test]
    fn authoring_disposition_dto_serializes_stably() {
        let d = AuthoringDispositionDto {
            state: DispositionState::SavedNeedsOperator,
            agent_terminal: true,
            remedies: vec![RemedyDto::set_attended("r", "abc123")],
            blocked_by: vec!["UNRESOLVED_REF".into()],
            acceptable_warnings: vec!["NO_RIG_CONFIGURED".into()],
            advisories: vec!["NO_TERMINAL_PATH".into()],
            completion: None,
        };
        let j = serde_json::to_value(&d).unwrap();
        // Null-noise discipline: absent, not null, on non-Valid states.
        assert!(j.get("completion").is_none());
        assert_eq!(j["state"], "saved-needs-operator");
        assert_eq!(j["agent_terminal"], true);
        assert_eq!(j["remedies"][0]["tool"], "routines_meta_set");
        assert_eq!(j["remedies"][0]["expected_revision"], "abc123");
        assert_eq!(j["remedies"][0]["actor"], "agent");
        assert_eq!(j["blocked_by"][0], "UNRESOLVED_REF");
        assert_eq!(j["acceptable_warnings"][0], "NO_RIG_CONFIGURED");
        assert_eq!(j["advisories"][0], "NO_TERMINAL_PATH");
    }

    /// The stop signal rides the wire exactly when the state is `Valid`
    /// (Laguna P1 2026-07-28: 37 polish edits against a green routine because
    /// nothing ever said "done"). Warnings-only findings still classify Valid
    /// and still carry it.
    #[test]
    fn valid_disposition_carries_completion_sentence() {
        let warn = FindingDto {
            code: "ATTENDED_UNDER_SCHEDULE".into(),
            severity: FindingSeverityDto::Warning,
            routine: "r".into(),
            track: None,
            step: None,
            message: "attended under schedule".into(),
        };
        let d = AuthoringDispositionDto::classify(&[warn], "r", "rev1");
        assert!(matches!(d.state, DispositionState::Valid));
        let c = d
            .completion
            .as_ref()
            .expect("Valid must carry the completion sentence");
        assert!(c.contains("COMPLETE"), "must state completion: {c}");
        assert!(
            c.contains("do not make further edits"),
            "must forbid warning-driven edits: {c}"
        );
        let j = serde_json::to_value(&d).unwrap();
        assert!(j["completion"].as_str().is_some(), "must serialize on Valid");
    }

    // --- tuxlink-lnctz: blocked_by / acceptable_warnings -------------------

    #[test]
    fn blocked_by_names_only_the_error_findings_and_dedupes() {
        // The Ladder-2 base/S4/rev_off finding set: one blocking
        // UNRESOLVED_REF among six warnings, several repeated. The model
        // rewrote control flow for 39 calls without ever addressing the one
        // finding that actually held the routine.
        let d = AuthoringDispositionDto::classify(
            &[
                warn("NO_RIG_CONFIGURED", "s2"),
                warn("NO_RIG_CONFIGURED", "s3"),
                warn("NO_TERMINAL_PATH", "..."),
                err("UNRESOLVED_REF", "step \"s2\" references @preset:40m-digital"),
                warn("ARM_FALLTHROUGH_LEAK", "..."),
            ],
            "r",
            "rev1",
        );
        assert_eq!(d.blocked_by, vec!["UNRESOLVED_REF".to_string()]);
        assert_eq!(
            d.acceptable_warnings,
            vec!["NO_RIG_CONFIGURED".to_string()],
            "deduped, environmental warnings only"
        );
        assert_eq!(
            d.advisories,
            vec![
                "NO_TERMINAL_PATH".to_string(),
                "ARM_FALLTHROUGH_LEAK".to_string(),
            ],
            "repairable structural warnings split out, first-seen order (tuxlink-0hjm4)"
        );
        assert_eq!(d.state, DispositionState::SavedNeedsOperator);
        assert!(d.remedies.is_empty(), "still no fabricated remedy");
    }

    // --- tuxlink-0hjm4: advisories vs acceptable_warnings ------------------

    #[test]
    fn structural_warnings_are_advisories_and_swap_the_completion_sentence() {
        // lift1-base E2: OUTPUT_NEVER_CONSUMED was on the wire and the model
        // shipped the dead read anyway. Declaring the routine COMPLETE over a
        // repairable structural warning is the coaching that permits that.
        let d = AuthoringDispositionDto::classify(
            &[
                warn("OUTPUT_NEVER_CONSUMED", "dead read"),
                warn("NO_RIG_CONFIGURED", "no rig"),
            ],
            "r",
            "rev1",
        );
        assert_eq!(d.state, DispositionState::Valid, "advisories never block");
        assert_eq!(d.advisories, vec!["OUTPUT_NEVER_CONSUMED".to_string()]);
        assert_eq!(d.acceptable_warnings, vec!["NO_RIG_CONFIGURED".to_string()]);
        let c = d.completion.as_ref().expect("Valid still carries a sentence");
        assert!(
            !c.contains("COMPLETE"),
            "must NOT declare completion over a repairable defect: {c}"
        );
        assert!(c.contains("advisories"), "must point at the advisories list: {c}");
        assert!(
            c.contains("repairable"),
            "must state the defects are repairable by editing: {c}"
        );
    }

    #[test]
    fn unknown_param_is_a_repairable_advisory_not_environmental() {
        // Surface-repair row 9 (zqo AS-CATALOG-ROUNDTRIP a1): an undeclared
        // param means the step silently drops part of its payload — that is
        // an authoring defect an edit fixes, and filing it under
        // acceptable_warnings had the completion prose calling it
        // unrepairable, which coached ignoring it.
        let d = AuthoringDispositionDto::classify(
            &[warn("UNKNOWN_PARAM", "param \"message\" is not declared")],
            "r",
            "rev1",
        );
        assert_eq!(d.state, DispositionState::Valid, "advisories never block");
        assert_eq!(d.advisories, vec!["UNKNOWN_PARAM".to_string()]);
        assert!(d.acceptable_warnings.is_empty());
        let c = d.completion.as_ref().expect("Valid still carries a sentence");
        assert!(!c.contains("COMPLETE"), "not complete over a dropped payload: {c}");
    }

    #[test]
    fn environmental_warnings_alone_keep_the_complete_sentence() {
        let d = AuthoringDispositionDto::classify(
            &[warn("NO_RIG_CONFIGURED", "no rig")],
            "r",
            "rev1",
        );
        assert_eq!(d.state, DispositionState::Valid);
        assert!(d.advisories.is_empty());
        let c = d.completion.as_ref().unwrap();
        assert!(c.contains("COMPLETE"), "{c}");
    }

    #[test]
    fn new_structural_lints_classify_as_advisories() {
        let d = AuthoringDispositionDto::classify(
            &[
                warn("REPEAT_CONNECT_NO_DELAY", "back to back dials"),
                warn("CONNECT_NOTHING_STAGED", "empty outbox flush"),
                warn("ARM_END_INVERTED", "failure masked"),
            ],
            "r",
            "rev1",
        );
        assert_eq!(
            d.advisories,
            vec![
                "REPEAT_CONNECT_NO_DELAY".to_string(),
                "CONNECT_NOTHING_STAGED".to_string(),
                "ARM_END_INVERTED".to_string(),
            ]
        );
        assert!(d.acceptable_warnings.is_empty());
    }

    #[test]
    fn advisory_and_completion_copy_is_ascii_clean() {
        // Operator ruling 2026-07-29: agent-facing wire strings must survive
        // non-UTF8-clean harness hops; an em-dash mid-instruction becomes
        // mojibake. Pin every disposition sentence to ASCII.
        for s in [valid_completion().unwrap(), advisory_completion().unwrap()] {
            assert!(s.is_ascii(), "non-ASCII in wire copy: {s}");
        }
    }

    #[test]
    fn a_valid_routine_reports_no_blocker_and_names_its_acceptable_warnings() {
        let d = AuthoringDispositionDto::classify(&[warn("ATTENDED_UNDER_SCHEDULE", "...")], "r", "rev1");
        assert_eq!(d.state, DispositionState::Valid);
        assert!(d.blocked_by.is_empty(), "Valid means nothing blocks");
        assert_eq!(d.acceptable_warnings, vec!["ATTENDED_UNDER_SCHEDULE".to_string()]);
        assert!(d.remedies.is_empty(), "acceptable warnings still get no remedy");
    }

    #[test]
    fn a_clean_routine_reports_both_lists_empty() {
        let d = AuthoringDispositionDto::classify(&[], "r", "rev1");
        assert!(d.blocked_by.is_empty());
        assert!(d.acceptable_warnings.is_empty());
    }

    #[test]
    fn operator_remedy_names_no_tool() {
        let j = serde_json::to_value(RemedyDto::operator_acknowledge("r")).unwrap();
        assert!(j.get("tool").is_none(), "operator remedy must not name an agent tool: {j}");
        assert_eq!(j["actor"], "operator");
    }

    #[test]
    fn automatic_unacked_transmit_is_saved_needs_operator_with_attended_alternative() {
        let d = AuthoringDispositionDto::classify(&[err("AUTO_TX_UNACKED", "...")], "r", "rev1");
        assert_eq!(d.state, DispositionState::SavedNeedsOperator);
        assert!(d.agent_terminal, "operator-gated states are terminal - the agent must stop, not loop");
        assert!(d
            .remedies
            .iter()
            .any(|r| matches!(r.actor, RemedyActor::Operator) && r.tool.is_none()));
        let attended = d.remedies.iter().find(|r| matches!(r.actor, RemedyActor::Agent)).unwrap();
        assert_eq!(attended.expected_revision.as_deref(), Some("rev1"), "revision-bound");
        assert!(attended.changes_behavior);
    }

    #[test]
    fn attended_under_schedule_warning_is_valid_terminal_not_a_loop() {
        let warn = FindingDto {
            code: "ATTENDED_UNDER_SCHEDULE".into(),
            severity: FindingSeverityDto::Warning,
            routine: "r".into(),
            track: None,
            step: None,
            message: String::new(),
        };
        let d = AuthoringDispositionDto::classify(&[warn], "r", "rev1");
        assert_eq!(d.state, DispositionState::Valid);
        assert!(d.remedies.is_empty(), "no remedy for an acceptable warning (kills the ping-pong)");
    }

    #[test]
    fn clean_routine_is_valid() {
        let d = AuthoringDispositionDto::classify(&[], "r", "rev1");
        assert_eq!(d.state, DispositionState::Valid);
        assert!(!d.agent_terminal);
    }

    #[test]
    fn callee_consent_unreachable_is_agent_repairable_targeting_the_callee() {
        let d = AuthoringDispositionDto::classify(
            &[err(
                "CALLEE_CONSENT_UNREACHABLE",
                "routine \"parent\" calls \"child\", which runs automatically...",
            )],
            "parent",
            "rev1",
        );
        assert_eq!(d.state, DispositionState::InvalidAgentRepairable);
        assert!(!d.agent_terminal);
        let r = &d.remedies[0];
        assert_eq!(r.routine.as_deref(), Some("child"), "remedy targets the offending callee");
        assert!(matches!(r.actor, RemedyActor::Agent));
    }

    // ── WritePortError as the disposition vocabulary (mutation-contract a) ──

    /// Only Invalid counts against the caller; only Unavailable invites a
    /// retry. Denied needs an operator act and Failed needs a human to read
    /// it — an agent looping on either is the guess-loop tuxlink-0rc3h
    /// removed.
    #[test]
    fn write_error_predicates_partition_the_variants() {
        let invalid = WritePortError::Invalid("bad".into());
        let denied = WritePortError::Denied("unarmed".into());
        let unavailable = WritePortError::Unavailable("backend offline".into());
        let failed = WritePortError::Failed("io".into());

        assert!(invalid.is_caller_attributable());
        assert!(!denied.is_caller_attributable());
        assert!(!unavailable.is_caller_attributable());
        assert!(!failed.is_caller_attributable());

        assert!(unavailable.is_retryable());
        assert!(!invalid.is_retryable());
        assert!(!denied.is_retryable());
        assert!(!failed.is_retryable());
    }

    /// The agent-visible text carries the classification: the reader is a
    /// model deciding what to do next, and "not your call's fault; retry
    /// later" is the instruction that stops it rewriting a correct call.
    /// The pre-existing prefixes stay byte-identical.
    #[test]
    fn write_error_display_teaches_the_next_move() {
        assert_eq!(
            WritePortError::Unavailable("backend offline".into()).to_string(),
            "unavailable right now (not your call's fault; retry later): backend offline"
        );
        assert_eq!(WritePortError::Denied("unarmed".into()).to_string(), "denied: unarmed");
        assert_eq!(WritePortError::Invalid("bad".into()).to_string(), "invalid: bad");
        assert_eq!(WritePortError::Failed("io".into()).to_string(), "failed: io");
    }
}
