//! Monolith-side implementations of the `tuxlink-mcp-core` port traits
//! (phase 3.2, Chunk 2).
//!
//! The MCP router in `tuxlink-mcp-core` is transport- and Tauri-free; it reads
//! live data exclusively through the [`StatusPort`]/[`MailboxPort`]/… traits.
//! This module supplies the REAL adapters: per-domain structs that each hold a
//! [`tauri::AppHandle`], fetch the relevant Tauri-managed state via
//! `self.app.state::<T>()` (or `try_state` where the state may be absent), call
//! the existing command-layer logic, and curate the monolith DTO into the
//! agent-facing mcp-core DTO.
//!
//! **Redaction is this module's job.** RAW values never cross the port
//! boundary: the grid is precision-reduced to a 4-char Maidenhead locator, the
//! session log's Wire-source lines are run through
//! [`crate::winlink::redaction::redact_wire_line`], and Bluetooth MACs are
//! minimized by [`minimize_bt_mac`] before the DTO is returned. The mcp-core
//! DTOs carry no password/secret fields by construction; the impls here never
//! populate one.
//!
//! Managed-state access pattern: a Tauri command receives `State<'_, T>` via the
//! invoke extractor. Outside a command we recover the same handle with
//! `AppHandle::state::<T>()` (panics if unmanaged) or `try_state::<T>()`
//! (returns `Option`, used for the optionally-managed `SearchService`). The
//! returned guard derefs to `&T`, so the existing logic (`state.snapshot()`,
//! `svc.run(spec)`, …) is reused verbatim.

use std::sync::Arc;

use async_trait::async_trait;
use tauri::{AppHandle, Manager};

// `WinlinkBackend` trait must be in scope to call its methods
// (`list_messages`, `read_message_in`, …) on the `Arc<dyn WinlinkBackend>`
// returned by `BackendState::current()`.

use tuxlink_mcp_core::ports::{
    AbortPort, ArdopConfigDto, AttachmentMetaDto, AudioDevicesDto, BackendStatusDto,
    BluetoothDeviceDto, CatalogEntryDto, ConfigPort, ConfigViewDto, DevicePort, DocsHitDto,
    EgressPort, EgressPortError, FolderDto, LogLineDto, LogPort, MailboxPort, MessageMetaDto,
    ModemStatusDto, PacketConfigDto, ParsedMessageDto, PlatformInfoDto, PortError,
    PositionStatusDto, SearchPort, SearchQueryDto, SearchResultsDto, SerialDeviceDto,
    SessionIntentDto, StatusPort, VaraConfigDto, VaraStatusDto,
};
use tuxlink_security::{guarded_egress, EgressAudit, EgressAuthority, EgressGuard};

// ---------------------------------------------------------------------------
// Bluetooth MAC minimization (Step 2).
// ---------------------------------------------------------------------------

/// Reduce a Bluetooth MAC to a low-fingerprint form for the read tier.
///
/// Rule: keep the FIRST and LAST octet, mask the four middle octets with `**`.
/// A full 48-bit address uniquely fingerprints a radio (and the OUI alone can
/// identify the manufacturer); keeping only the first octet plus the last octet
/// lets an operator distinguish two paired devices in a listing without
/// exposing an address an agent could log, correlate, or exfiltrate. The first
/// octet is the most-significant byte of the OUI (a coarse vendor hint, not a
/// vendor identification); the last octet is the most volatile byte across
/// devices of the same model.
///
/// Input that is not a canonical 6-octet colon-separated MAC is passed through
/// unchanged EXCEPT that any `:`-separated middle segments are still masked when
/// there are at least three segments; a value with fewer than three segments is
/// returned verbatim (it is already too short to fingerprint a device).
pub fn minimize_bt_mac(mac: &str) -> String {
    let parts: Vec<&str> = mac.split(':').collect();
    if parts.len() < 3 {
        return mac.to_string();
    }
    let mut out: Vec<String> = Vec::with_capacity(parts.len());
    let last = parts.len() - 1;
    for (i, seg) in parts.iter().enumerate() {
        if i == 0 || i == last {
            out.push((*seg).to_string());
        } else {
            out.push("**".to_string());
        }
    }
    out.join(":")
}

// ---------------------------------------------------------------------------
// Status port.
// ---------------------------------------------------------------------------

/// [`StatusPort`] adapter over the monolith's backend/modem/VARA/position state.
pub struct MonolithStatusPort {
    app: AppHandle,
}

impl MonolithStatusPort {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

#[async_trait]
impl StatusPort for MonolithStatusPort {
    async fn backend_status(&self) -> Result<BackendStatusDto, PortError> {
        use crate::ui_commands::StatusDto;
        let state = self.app.state::<crate::app_backend::BackendState>();
        // `snapshot()` clones (phase, backend) under one read guard and drops
        // it; `derive_status_dto` is pure (mirrors the `backend_status`
        // command).
        let (phase, backend) = state.snapshot();
        let dto = crate::ui_commands::derive_status_dto(phase, backend);
        // Curate the tagged StatusDto enum into the flat {connected, transport,
        // state} agent shape. `None` (NotConfigured) → disconnected/idle.
        let curated = match dto {
            None => BackendStatusDto {
                connected: false,
                transport: String::new(),
                state: "not_configured".to_string(),
            },
            Some(StatusDto::Disconnected) => BackendStatusDto {
                connected: false,
                transport: String::new(),
                state: "disconnected".to_string(),
            },
            Some(StatusDto::Connecting { transport }) => BackendStatusDto {
                connected: false,
                transport,
                state: "connecting".to_string(),
            },
            Some(StatusDto::Listening { transport }) => BackendStatusDto {
                connected: false,
                transport,
                state: "listening".to_string(),
            },
            Some(StatusDto::Connected { transport, .. }) => BackendStatusDto {
                connected: true,
                transport,
                state: "connected".to_string(),
            },
            Some(StatusDto::Disconnecting) => BackendStatusDto {
                connected: false,
                transport: String::new(),
                state: "disconnecting".to_string(),
            },
            Some(StatusDto::Error { reason }) => BackendStatusDto {
                connected: false,
                transport: String::new(),
                state: format!("error: {reason}"),
            },
        };
        Ok(curated)
    }

    async fn modem_status(&self) -> Result<ModemStatusDto, PortError> {
        use crate::modem_status::ModemState;
        let session = self
            .app
            .state::<Arc<crate::modem_status::ModemSession>>();
        let status = crate::modem_commands::modem_get_status_inner(session.inner());
        let connected = matches!(
            status.state,
            ModemState::ConnectedIrs | ModemState::ConnectedIss
        );
        // Lower-case Debug rendering of the state enum for a stable string.
        let state = format!("{:?}", status.state).to_lowercase();
        Ok(ModemStatusDto {
            kind: "ardop".to_string(),
            connected,
            state,
        })
    }

    async fn vara_status(&self) -> Result<VaraStatusDto, PortError> {
        use crate::winlink::modem::vara::commands::VaraState;
        let session = self
            .app
            .state::<Arc<crate::winlink::modem::vara::VaraSession>>();
        // The `vara_status` command body is `session.snapshot()`; reuse it.
        let status = session.snapshot();
        let connected = matches!(status.state, VaraState::Open);
        // VARA bandwidth lives in config, not the live status; surface the
        // configured bandwidth so the agent sees the negotiated width target.
        let bandwidth = crate::winlink::modem::vara::commands::config_get_vara()
            .bandwidth_hz
            .unwrap_or(0);
        let state = format!("{:?}", status.state).to_lowercase();
        Ok(VaraStatusDto {
            connected,
            bandwidth,
            state,
        })
    }

    async fn position_status(&self) -> Result<PositionStatusDto, PortError> {
        use crate::config::{PositionPrecision, PositionSource};
        let arbiter_state = self
            .app
            .state::<Arc<crate::position::PositionArbiter>>();
        // `effective_broadcast_locator` wants `Option<&PositionArbiter>`; deref
        // the State→Arc→PositionArbiter chain to a plain reference (State derefs
        // to Arc, Arc derefs to PositionArbiter).
        let arbiter: &crate::position::PositionArbiter = &arbiter_state;
        let cfg = crate::config::read_config()
            .map_err(|e| PortError::Internal(format!("{e:?}")))?;
        let has_fix = arbiter.has_fresh_fix()
            && cfg.privacy.gps_state != crate::config::GpsState::Off;
        // Reduce the broadcast locator to a 4-char grid for the MCP DTO —
        // privacy default (the GUI keeps full precision; the agent surface does
        // not). `effective_broadcast_locator` already honors gps_state; we
        // additionally clamp precision to FourCharGrid here.
        let raw_grid =
            crate::position::effective_broadcast_locator(&cfg, Some(arbiter));
        let grid = crate::config::broadcast_grid(&raw_grid, PositionPrecision::FourCharGrid);
        let source = match cfg.privacy.position_source {
            PositionSource::Gps => "gps".to_string(),
            PositionSource::Manual => "manual".to_string(),
        };
        Ok(PositionStatusDto {
            has_fix,
            grid,
            source,
        })
    }

    async fn platform_info(&self) -> Result<PlatformInfoDto, PortError> {
        // Pure; no managed state. Echo the app version the embedder built with.
        let info = crate::winlink::modem::vara::commands::platform_info();
        Ok(PlatformInfoDto {
            os: info.os,
            arch: info.arch,
            app_version: env!("CARGO_PKG_VERSION").to_string(),
        })
    }

    async fn wizard_completed(&self) -> Result<bool, PortError> {
        crate::wizard::get_wizard_completed()
            .await
            .map_err(|e| PortError::Internal(format!("{e:?}")))
    }

    async fn p2p_peer_password_status(&self, callsign: &str) -> Result<bool, PortError> {
        use crate::ui_commands::PeerPasswordStatus;
        let status = crate::ui_commands::p2p_peer_password_status(callsign.to_string())
            .await
            .map_err(|e| PortError::Internal(format!("{e:?}")))?;
        // Return ONLY the set/not-set boolean — never the password.
        Ok(matches!(status, PeerPasswordStatus::Set))
    }
}

// ---------------------------------------------------------------------------
// Mailbox port.
// ---------------------------------------------------------------------------

/// [`MailboxPort`] adapter over the native mailbox backend.
pub struct MonolithMailboxPort {
    app: AppHandle,
}

impl MonolithMailboxPort {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }

    fn backend(
        &self,
    ) -> Result<Arc<dyn crate::winlink_backend::WinlinkBackend>, PortError> {
        let state = self.app.state::<crate::app_backend::BackendState>();
        state
            .current()
            .ok_or_else(|| PortError::Unavailable("backend offline".to_string()))
    }
}

/// Map a monolith mailbox `MessageMetaDto` onto the mcp-core shape (Vec<String>
/// `to` joined to a single comma string; no body_size/identity in the agent
/// shape).
fn map_message_meta(m: crate::ui_commands::MessageMetaDto) -> MessageMetaDto {
    MessageMetaDto {
        id: m.id,
        subject: m.subject,
        from: m.from,
        to: m.to.join(", "),
        date: m.date,
        unread: m.unread,
        has_attachments: m.has_attachments,
    }
}

#[async_trait]
impl MailboxPort for MonolithMailboxPort {
    async fn list(&self, folder: &str) -> Result<Vec<MessageMetaDto>, PortError> {
        let backend = self.backend()?;
        let parsed = crate::ui_commands::parse_folder_ref(folder)
            .map_err(|e| PortError::Internal(format!("{e:?}")))?;
        let metas = crate::ui_core::mailbox::list_mailbox(&backend, parsed)
            .await
            .map_err(|e| PortError::Internal(format!("{e:?}")))?;
        Ok(metas.into_iter().map(map_message_meta).collect())
    }

    async fn read(&self, folder: &str, id: &str) -> Result<ParsedMessageDto, PortError> {
        use crate::native_mailbox::FolderRef;
        use crate::winlink_backend::MessageId;
        let backend = self.backend()?;
        let parsed = crate::ui_commands::parse_folder_ref(folder)
            .map_err(|e| PortError::Internal(format!("{e:?}")))?;
        let mid = MessageId::new(id);
        // Same fetch path as the `message_read` command.
        let body = match &parsed {
            FolderRef::System(f) => backend
                .read_message_in(*f, &mid)
                .await
                .map_err(|e| PortError::Internal(format!("{e:?}")))?,
            FolderRef::User(slug) => backend
                .read_user_message(slug, &mid)
                .await
                .map_err(|e| PortError::Internal(format!("{e:?}")))?,
        };
        let dto = crate::ui_commands::parse_raw_rfc5322(id, &body.raw_rfc5322)
            .map_err(|e| PortError::Internal(format!("{e:?}")))?;
        Ok(ParsedMessageDto {
            id: dto.id,
            subject: dto.subject,
            from: dto.from,
            to: dto.to.join(", "),
            cc: dto.cc.join(", "),
            date: dto.date,
            body: dto.body,
            attachments: dto
                .attachments
                .into_iter()
                .map(|a| AttachmentMetaDto {
                    filename: a.filename,
                    size: a.size,
                })
                .collect(),
            has_form: dto.is_form,
        })
    }

    async fn folders(&self) -> Result<Vec<FolderDto>, PortError> {
        use crate::winlink_backend::MailboxFolder;
        let backend = self.backend()?;
        let mut out: Vec<FolderDto> = Vec::new();
        // System folders + their message counts. There is no count API on the
        // backend trait; the count is the length of a folder listing.
        let system = [
            ("inbox", MailboxFolder::Inbox),
            ("outbox", MailboxFolder::Outbox),
            ("sent", MailboxFolder::Sent),
            ("archive", MailboxFolder::Archive),
            ("deleted", MailboxFolder::Deleted),
        ];
        for (name, folder) in system {
            let metas = backend
                .list_messages(folder)
                .await
                .map_err(|e| PortError::Internal(format!("{e:?}")))?;
            out.push(FolderDto {
                name: name.to_string(),
                count: u32::try_from(metas.len()).unwrap_or(u32::MAX),
            });
        }
        // User-created folders.
        let user = backend
            .list_user_folders()
            .await
            .map_err(|e| PortError::Internal(format!("{e:?}")))?;
        for f in user {
            let metas = backend
                .list_user_messages(&f.slug)
                .await
                .map_err(|e| PortError::Internal(format!("{e:?}")))?;
            out.push(FolderDto {
                name: f.display_name,
                count: u32::try_from(metas.len()).unwrap_or(u32::MAX),
            });
        }
        Ok(out)
    }
}

// ---------------------------------------------------------------------------
// Search port.
// ---------------------------------------------------------------------------

/// [`SearchPort`] adapter over the find-messages `SearchService` + docs/catalog.
pub struct MonolithSearchPort {
    app: AppHandle,
}

impl MonolithSearchPort {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

#[async_trait]
impl SearchPort for MonolithSearchPort {
    async fn messages(&self, query: SearchQueryDto) -> Result<SearchResultsDto, PortError> {
        use crate::search::types::{FilterKey, FilterValue, PageRequest, QuerySpec};
        // SearchService is OPTIONALLY managed (build_service failure at startup
        // leaves it unmanaged); the command extractor would panic, so use
        // try_state and degrade to Unavailable.
        let svc = self
            .app
            .try_state::<crate::search::commands::SearchService>()
            .ok_or_else(|| PortError::Unavailable("search index unavailable".to_string()))?;
        let mut spec = QuerySpec {
            free_text: if query.query.is_empty() {
                None
            } else {
                Some(query.query.clone())
            },
            ..QuerySpec::default()
        };
        if let Some(folder) = query.folder {
            spec.filters
                .insert(FilterKey::Folder, FilterValue::Folder(folder));
        }
        if let Some(limit) = query.limit {
            spec.page = PageRequest {
                page_size: limit,
                offset: 0,
            };
        }
        let results = svc
            .run(spec)
            .map_err(|e| PortError::Internal(format!("{e:?}")))?;
        Ok(SearchResultsDto {
            items: results
                .items
                .into_iter()
                .map(|m| MessageMetaDto {
                    id: m.id,
                    subject: m.subject,
                    from: m.from,
                    to: m.to.join(", "),
                    date: m.date,
                    unread: m.unread,
                    has_attachments: m.has_attachments,
                })
                .collect(),
            total: results.total_matches,
        })
    }

    async fn docs(&self, query: &str) -> Result<Vec<DocsHitDto>, PortError> {
        let svc = self
            .app
            .try_state::<crate::search::commands::SearchService>()
            .ok_or_else(|| PortError::Unavailable("search index unavailable".to_string()))?;
        // Mirror `docs_search`: lock the shared index and run the docs FTS path.
        let hits = svc
            .index
            .lock()
            .map_err(|e| PortError::Internal(format!("docs index poisoned: {e}")))?
            .search_docs(query)
            .map_err(|e| PortError::Internal(format!("{e:?}")))?;
        Ok(hits
            .into_iter()
            .map(|h| DocsHitDto {
                title: h.title,
                path: h.slug,
                snippet: h.snippet,
            })
            .collect())
    }

    async fn catalog(&self) -> Result<Vec<CatalogEntryDto>, PortError> {
        // Pure: parses the bundled catalog; no managed state.
        let entries = crate::catalog::commands::catalog_list()
            .map_err(|e| PortError::Internal(format!("{e:?}")))?;
        Ok(entries
            .into_iter()
            .map(|e| CatalogEntryDto {
                id: e.filename,
                title: e.description,
                category: e.category,
            })
            .collect())
    }
}

// ---------------------------------------------------------------------------
// Config port.
// ---------------------------------------------------------------------------

/// [`ConfigPort`] adapter over the config-view + per-modem config readers.
pub struct MonolithConfigPort {
    #[allow(dead_code)]
    app: AppHandle,
}

impl MonolithConfigPort {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

#[async_trait]
impl ConfigPort for MonolithConfigPort {
    async fn read(&self) -> Result<ConfigViewDto, PortError> {
        // `redact_config_view` reduces the grid to a 4-char locator via
        // `broadcast_grid(.., FourCharGrid)` — the redaction boundary. Read the
        // raw view, then redact BEFORE crossing the port.
        let raw = crate::ui_core::config::read_config_view()
            .map_err(|e| PortError::Internal(format!("{e:?}")))?;
        let view = crate::ui_core::config::redact_config_view(raw);
        Ok(ConfigViewDto {
            connect_to_cms: view.connect_to_cms,
            // CmsTransport → its string form (Debug is the stable label the
            // frontend's normalizeTransportLabel consumes).
            transport: format!("{:?}", view.transport),
            host: view.host,
            callsign: view.callsign.unwrap_or_default(),
            grid: view.grid.unwrap_or_default(),
        })
    }

    async fn ardop(&self) -> Result<ArdopConfigDto, PortError> {
        let cfg = crate::modem_commands::config_get_ardop();
        Ok(ArdopConfigDto {
            // ARDOP cmd port is local; ArdopUiConfig carries no host field.
            host: "127.0.0.1".to_string(),
            port: cfg.cmd_port,
            drive_level: cfg.drive_level.unwrap_or(0),
            bandwidth: cfg.bandwidth_hz.unwrap_or(0),
        })
    }

    async fn vara(&self) -> Result<VaraConfigDto, PortError> {
        let cfg = crate::winlink::modem::vara::commands::config_get_vara();
        Ok(VaraConfigDto {
            host: cfg.host,
            port: cfg.cmd_port,
            bandwidth: cfg.bandwidth_hz.unwrap_or(0),
            // VARA has no client-side drive-level config (the modem app owns
            // TX level); surface 0 as "not applicable".
            drive_level: 0,
        })
    }

    async fn packet(&self) -> Result<PacketConfigDto, PortError> {
        let cfg = crate::ui_commands::packet_config_get()
            .await
            .map_err(|e| PortError::Internal(format!("{e:?}")))?;
        // The bt_mac field is minimized before crossing the boundary even though
        // PacketConfigDto's mcp-core shape has no MAC field today — the packet
        // DTO surfaces the KISS-link parameters only. If a future mcp-core
        // PacketConfigDto adds a bt_mac field, minimize it here:
        //   let _bt = cfg.bt_mac.as_deref().map(minimize_bt_mac);
        Ok(PacketConfigDto {
            kiss_host: cfg.tcp_host.unwrap_or_default(),
            kiss_port: cfg.tcp_port.unwrap_or(0),
            baud: cfg.serial_baud.unwrap_or(0),
            tx_delay: u32::from(cfg.txdelay),
        })
    }
}

// ---------------------------------------------------------------------------
// Device port.
// ---------------------------------------------------------------------------

/// [`DevicePort`] adapter over the serial/Bluetooth/audio device enumerators.
pub struct MonolithDevicePort {
    #[allow(dead_code)]
    app: AppHandle,
}

impl MonolithDevicePort {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

#[async_trait]
impl DevicePort for MonolithDevicePort {
    async fn serial(&self) -> Result<Vec<SerialDeviceDto>, PortError> {
        let devices = crate::ui_commands::packet_list_serial_devices()
            .await
            .map_err(|e| PortError::Internal(format!("{e:?}")))?;
        Ok(devices
            .into_iter()
            .map(|d| SerialDeviceDto {
                path: d.path,
                description: d.label,
            })
            .collect())
    }

    async fn bluetooth(&self) -> Result<Vec<BluetoothDeviceDto>, PortError> {
        let devices = crate::ui_commands::packet_list_bluetooth_devices()
            .await
            .map_err(|e| PortError::Internal(format!("{e:?}")))?;
        Ok(devices
            .into_iter()
            .map(|d| BluetoothDeviceDto {
                name: d.name,
                // MINIMIZE the MAC at the boundary — never expose the full
                // address to the agent surface.
                mac: minimize_bt_mac(&d.mac),
            })
            .collect())
    }

    async fn audio(&self) -> Result<AudioDevicesDto, PortError> {
        let devices = crate::ui_commands::ardop_list_audio_devices()
            .await
            .map_err(|e| PortError::Internal(format!("{e:?}")))?;
        Ok(AudioDevicesDto {
            capture: devices.captures.into_iter().map(|d| d.name).collect(),
            playback: devices.playbacks.into_iter().map(|d| d.name).collect(),
        })
    }
}

// ---------------------------------------------------------------------------
// Log port.
// ---------------------------------------------------------------------------

/// [`LogPort`] adapter over the session-log snapshot, redacting Wire lines.
pub struct MonolithLogPort {
    app: AppHandle,
}

impl MonolithLogPort {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

#[async_trait]
impl LogPort for MonolithLogPort {
    async fn snapshot(&self) -> Result<Vec<LogLineDto>, PortError> {
        use crate::winlink_backend::{LogLevel, LogSource};
        let state = self
            .app
            .state::<Arc<crate::session_log::SessionLogState>>();
        // `SessionLogState::snapshot` returns the durable `Vec<LogLine>`
        // (the `session_log_snapshot` command maps these to `LogLineDto`).
        let lines = state.snapshot();
        Ok(lines
            .into_iter()
            .map(|l| {
                // Redact credential tokens (;PQ/;PR) on Wire-source lines BEFORE
                // the line crosses into mcp-core. Backend/Transport lines are
                // operator-visible app events and are not wire bytes. `LogSource`
                // is matched exhaustively in-crate; if a Wire-class variant is
                // ever added, add it to the redacting arm (fail safe).
                let message = match l.source {
                    LogSource::Backend | LogSource::Transport => l.message,
                    LogSource::Wire => {
                        crate::winlink::redaction::redact_wire_line(&l.message).into_owned()
                    }
                };
                let level = match l.level {
                    LogLevel::Trace => "trace",
                    LogLevel::Debug => "debug",
                    LogLevel::Info => "info",
                    LogLevel::Warn => "warn",
                    LogLevel::Error => "error",
                };
                LogLineDto {
                    timestamp: l.timestamp_iso,
                    level: level.to_string(),
                    message,
                }
            })
            .collect())
    }
}

// ---------------------------------------------------------------------------
// Egress port (phase 3.3) — GATED Agent egress.
//
// Every method runs the REAL egress (the same connect/exchange the GUI command
// performs) INSIDE `guarded_egress(.., EgressAuthority::Agent, ..)`. The gate is
// the operator's live arm/taint/poison state shared via the `Arc<EgressGuard>`
// the monolith manages (same Arc the GUI's egress_arm/egress_disarm mutate). A
// disarmed / expired / tainted / poisoned session yields
// `EgressDenied` → `EgressPortError::Denied`; an authorized op that then fails
// operationally yields `EgressPortError::Failed`. The GUI Tauri commands are
// UNCHANGED (Operator is unconditionally allowed and never reaches this path).
//
// Audit: each op builds a small `Fn(EgressAudit)` closure capturing a cloned
// `AppHandle` that writes one operator-visible session-log line (Info on allow,
// Warn on deny) AND a structured `tracing` record under
// `target: "tuxlink::egress_gate"`. The AppHandle is `Clone`; the closure is
// `Send + Sync` (it captures only `Clone` handles + `&str`/`String`), satisfying
// `guarded_egress`'s `&(dyn Fn(EgressAudit<'_>) + Send + Sync)` bound.
// ---------------------------------------------------------------------------

/// [`EgressPort`] adapter that gates each Agent egress through the shared
/// [`EgressGuard`] before performing the same connect/exchange the GUI command
/// performs.
pub struct MonolithEgressPort {
    app: AppHandle,
    guard: Arc<EgressGuard>,
}

impl MonolithEgressPort {
    pub fn new(app: AppHandle, guard: Arc<EgressGuard>) -> Self {
        Self { app, guard }
    }
}

/// Build the audit sink for one gated egress: writes an operator-visible
/// session-log line (Warn on denial, Info on allow) and a structured tracing
/// record. Returned as an owned closure so each `guarded_egress` call passes it
/// by reference. Captures only `Clone` handles → `Send + Sync`.
fn egress_audit_sink(app: AppHandle) -> impl Fn(EgressAudit<'_>) + Send + Sync {
    use crate::winlink_backend::{LogLevel, LogSource};
    move |a: EgressAudit<'_>| {
        let log = app.state::<Arc<crate::session_log::SessionLogState>>();
        if a.allowed {
            let msg = format!("[egress] {} authorized for Agent", a.op);
            tracing::info!(
                target: "tuxlink::egress_gate",
                op = a.op,
                authority = ?a.authority,
                allowed = true,
                "agent egress authorized"
            );
            log.append_operator_line(LogLevel::Info, LogSource::Backend, msg);
        } else {
            let reason = a.reason.as_deref().unwrap_or("denied");
            let msg = format!("[egress] {} DENIED for Agent: {reason}", a.op);
            tracing::warn!(
                target: "tuxlink::egress_gate",
                op = a.op,
                authority = ?a.authority,
                allowed = false,
                reason = reason,
                "agent egress denied"
            );
            log.append_operator_line(LogLevel::Warn, LogSource::Backend, msg);
        }
    }
}

/// Map a `SessionIntentDto` onto the monolith's [`SessionIntent`] 1:1.
///
/// The agent-facing DTO mirrors `SessionIntent`'s routing-pool variants
/// (Cms / RadioOnly / PostOffice / Mesh / P2p), so this is a faithful
/// variant-for-variant map — the agent selects the same message pool the GUI
/// would. A B2F exchange always performs a full send+receive round once
/// connected; the intent selects routing, not transfer direction.
///
/// [`SessionIntent`]: crate::winlink::session::SessionIntent
fn map_session_intent(
    intent: SessionIntentDto,
) -> crate::winlink::session::SessionIntent {
    use crate::winlink::session::SessionIntent;
    match intent {
        SessionIntentDto::Cms => SessionIntent::Cms,
        SessionIntentDto::RadioOnly => SessionIntent::RadioOnly,
        SessionIntentDto::PostOffice => SessionIntent::PostOffice,
        SessionIntentDto::Mesh => SessionIntent::Mesh,
        SessionIntentDto::P2p => SessionIntent::P2p,
    }
}

#[async_trait]
impl EgressPort for MonolithEgressPort {
    async fn cms_connect(&self) -> Result<(), EgressPortError> {
        let audit = egress_audit_sink(self.app.clone());
        let app = self.app.clone();
        guarded_egress(&self.guard, EgressAuthority::Agent, "cms_connect", &audit, || async move {
            // Same egress as the `cms_connect` command (ui_commands.rs:2891):
            // drive `NativeBackend::connect` over the configured CMS transport
            // (the outbox flush rides inside the native exchange), then close
            // the transient session. Resolve managed state via the AppHandle.
            crate::ui_commands::cms_connect(
                app.clone(),
                app.state::<crate::app_backend::BackendState>(),
                app.state::<Arc<crate::session_log::SessionLogState>>(),
                app.state::<crate::winlink::inbound_selection::SelectionRegistry>(),
            )
            .await
            .map_err(|e| EgressPortError::Failed(format!("{e:?}")))
        })
        .await
        .map_err(|d| EgressPortError::Denied(d.to_string()))?
    }

    async fn verify_cms_connection(&self) -> Result<(), EgressPortError> {
        let audit = egress_audit_sink(self.app.clone());
        let app = self.app.clone();
        guarded_egress(
            &self.guard,
            EgressAuthority::Agent,
            "verify_cms_connection",
            &audit,
            || async move {
                // wizard.rs:479 verify_cms_connection_impl — ephemeral backend
                // over an empty tempdir outbox (handshake only). WizardError has
                // no Display → format with {e:?}.
                crate::wizard::verify_cms_connection_impl(app.clone())
                    .await
                    .map_err(|e| EgressPortError::Failed(format!("{e:?}")))
            },
        )
        .await
        .map_err(|d| EgressPortError::Denied(d.to_string()))?
    }

    async fn ardop_connect(&self, target: String) -> Result<(), EgressPortError> {
        let audit = egress_audit_sink(self.app.clone());
        let app = self.app.clone();
        guarded_egress(&self.guard, EgressAuthority::Agent, "ardop_connect", &audit, || async move {
            // modem_commands.rs:1123 modem_ardop_connect (Arc<ModemSession>).
            crate::modem_commands::modem_ardop_connect(
                app.clone(),
                app.state::<Arc<crate::modem_status::ModemSession>>(),
                target,
            )
            .await
            .map_err(EgressPortError::Failed)
        })
        .await
        .map_err(|d| EgressPortError::Denied(d.to_string()))?
    }

    async fn ardop_b2f_exchange(
        &self,
        target: String,
        intent: SessionIntentDto,
    ) -> Result<(), EgressPortError> {
        let audit = egress_audit_sink(self.app.clone());
        let app = self.app.clone();
        guarded_egress(
            &self.guard,
            EgressAuthority::Agent,
            "ardop_b2f_exchange",
            &audit,
            || async move {
                // modem_commands.rs:1253 modem_ardop_b2f_exchange — pin the
                // ARDOP TransportKind (the command validates it) and map the
                // coarse intent DTO onto SessionIntent.
                crate::modem_commands::modem_ardop_b2f_exchange(
                    app.clone(),
                    app.state::<Arc<crate::modem_status::ModemSession>>(),
                    target,
                    map_session_intent(intent),
                    crate::winlink::listener::transport::TransportKind::Ardop,
                )
                .await
                .map_err(EgressPortError::Failed)
            },
        )
        .await
        .map_err(|d| EgressPortError::Denied(d.to_string()))?
    }

    async fn vara_b2f_exchange(
        &self,
        target: String,
        intent: SessionIntentDto,
    ) -> Result<(), EgressPortError> {
        let audit = egress_audit_sink(self.app.clone());
        let app = self.app.clone();
        guarded_egress(
            &self.guard,
            EgressAuthority::Agent,
            "vara_b2f_exchange",
            &audit,
            || async move {
                // vara/commands.rs:1541 modem_vara_b2f_exchange — VARA CONNECT
                // is LIVE here; the gate runs the real path. Pin VARA-HF (the
                // operationally-confirmed G90 + VARA HF Standard path); the
                // command validates the kind is VaraHf | VaraFm.
                crate::winlink::modem::vara::commands::modem_vara_b2f_exchange(
                    app.clone(),
                    app.state::<Arc<crate::session_log::SessionLogState>>(),
                    app.state::<Arc<crate::winlink::modem::vara::VaraSession>>(),
                    target,
                    map_session_intent(intent),
                    crate::winlink::listener::transport::TransportKind::VaraHf,
                )
                .await
                .map_err(EgressPortError::Failed)
            },
        )
        .await
        .map_err(|d| EgressPortError::Denied(d.to_string()))?
    }

    async fn packet_connect(
        &self,
        call: String,
        path: Vec<String>,
    ) -> Result<(), EgressPortError> {
        let audit = egress_audit_sink(self.app.clone());
        let app = self.app.clone();
        guarded_egress(&self.guard, EgressAuthority::Agent, "packet_connect", &audit, || async move {
            // ui_commands.rs:4534 packet_connect.
            crate::ui_commands::packet_connect(
                app.clone(),
                app.state::<crate::app_backend::BackendState>(),
                app.state::<Arc<crate::session_log::SessionLogState>>(),
                call,
                path,
            )
            .await
            .map_err(|e| EgressPortError::Failed(format!("{e:?}")))
        })
        .await
        .map_err(|d| EgressPortError::Denied(d.to_string()))?
    }
}

// ---------------------------------------------------------------------------
// Abort port (phase 3.3) — UNGATED pure-stop.
//
// Stopping a transmission/connection is ALWAYS allowed — never gated by
// armed/taint state — because a working abort is a safety primitive, not an
// egress. Each method calls the existing abort fn directly (no guarded_egress)
// and appends a forensic "[egress] abort <op> by Agent" session-log line; an
// abort is NEVER denied. Errors are operational only → `PortError::Internal`.
// ---------------------------------------------------------------------------

/// [`AbortPort`] adapter over the monolith's per-transport abort/stop fns.
pub struct MonolithAbortPort {
    app: AppHandle,
}

impl MonolithAbortPort {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }

    /// Append a forensic abort line to the operator-visible session log. Never
    /// fails the abort (abort is unconditional); a missing/poisoned log sink is
    /// silently tolerated.
    fn audit_abort(&self, op: &str) {
        use crate::winlink_backend::{LogLevel, LogSource};
        tracing::info!(
            target: "tuxlink::egress_gate",
            op = op,
            "agent abort"
        );
        let log = self.app.state::<Arc<crate::session_log::SessionLogState>>();
        log.append_operator_line(
            LogLevel::Info,
            LogSource::Backend,
            format!("[egress] abort {op} by Agent"),
        );
    }
}

#[async_trait]
impl AbortPort for MonolithAbortPort {
    async fn cms_abort(&self) -> Result<(), PortError> {
        self.audit_abort("cms_abort");
        // ui_commands.rs:3062 cms_abort — backend.abort() + wake parked decider.
        crate::ui_commands::cms_abort(
            self.app.clone(),
            self.app.state::<crate::app_backend::BackendState>(),
            self.app.state::<Arc<crate::session_log::SessionLogState>>(),
            self.app
                .state::<crate::winlink::inbound_selection::SelectionRegistry>(),
        )
        .await
        .map_err(|e| PortError::Internal(format!("{e:?}")))
    }

    async fn ardop_disconnect(&self) -> Result<(), PortError> {
        self.audit_abort("ardop_disconnect");
        // modem_commands.rs:202 modem_ardop_disconnect (inner :153) — best-effort
        // abort_in_flight + transport teardown.
        crate::modem_commands::modem_ardop_disconnect(
            self.app.clone(),
            self.app.state::<Arc<crate::modem_status::ModemSession>>(),
        )
        .await
        .map_err(PortError::Internal)
    }

    async fn vara_stop_session(&self) -> Result<(), PortError> {
        self.audit_abort("vara_stop_session");
        // vara/commands.rs:1420 vara_stop_session_inner(&Arc<VaraSession>) — the
        // sync transport-teardown helper; call it directly off the managed
        // VaraSession (the command wrapper only adds state extraction).
        let session = self
            .app
            .state::<Arc<crate::winlink::modem::vara::VaraSession>>();
        let session = Arc::clone(session.inner());
        crate::winlink::modem::vara::commands::vara_stop_session_inner(&session)
            .map(|_status| ())
            .map_err(PortError::Internal)
    }
}

#[cfg(test)]
mod tests {
    use super::minimize_bt_mac;

    #[test]
    fn minimize_bt_mac_masks_middle_octets() {
        // Canonical 6-octet MAC: keep first + last, mask the middle four.
        assert_eq!(
            minimize_bt_mac("38:D2:00:01:55:5C"),
            "38:**:**:**:**:5C"
        );
        assert_eq!(
            minimize_bt_mac("AA:BB:CC:DD:EE:FF"),
            "AA:**:**:**:**:FF"
        );
    }

    #[test]
    fn minimize_bt_mac_passes_through_short_input() {
        // Fewer than three segments cannot fingerprint a device → verbatim.
        assert_eq!(minimize_bt_mac("AA:BB"), "AA:BB");
        assert_eq!(minimize_bt_mac("AA"), "AA");
        assert_eq!(minimize_bt_mac(""), "");
    }

    #[test]
    fn minimize_bt_mac_masks_three_segment_input() {
        // Exactly three segments: first + last kept, single middle masked.
        assert_eq!(minimize_bt_mac("AA:BB:CC"), "AA:**:CC");
    }
}
