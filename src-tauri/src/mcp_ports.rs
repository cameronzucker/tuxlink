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
    AbortPort, ArdopConfigDto, ArdopWriteDto, AttachmentMetaDto, AudioDevicesDto, BackendStatusDto,
    BluetoothDeviceDto, CatalogEntryDto, ComposeDraftDto, ComposePort, ConfigPort, ConfigViewDto,
    DevicePort, DocsHitDto, EgressPort, EgressPortError, FolderDto, GribRequestDto, LogLineDto,
    LogPort, MailboxPort, MessageMetaDto, ModemStatusDto, PacketConfigDto, PacketWriteDto,
    ParsedMessageDto, PlatformInfoDto, PortError, PositionStatusDto, SearchPort, SearchQueryDto,
    SearchResultsDto, SendFormDto, SerialDeviceDto, SessionIntentDto, StatusPort, VaraConfigDto,
    VaraStatusDto, VaraWriteDto, WritePort, WritePortError,
};
use tuxlink_mcp_core::validate::{
    validate_address, validate_attachment_dest, validate_body, validate_drive_level,
    validate_subject, validate_vara_bandwidth,
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
            // modem_commands.rs modem_ardop_connect (Arc<ModemSession>).
            // rig-control Task 8/9: the MCP egress dial keeps the legacy
            // single-target, no-tune, no-QSY behavior — `freq_hz` + the QSY
            // candidate list are operator-UI concerns (Task 10), not agent
            // egress. `None, None` reproduces the pre-rig-control single dial.
            crate::modem_commands::modem_ardop_connect(
                app.clone(),
                app.state::<Arc<crate::modem_status::ModemSession>>(),
                target,
                None,
                None,
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
                    // The MCP egress path does not QSY: no pre-audio CAT tune
                    // (no rig freq known here) and no candidate list. Pass
                    // `None, None` for the tuxlink-8fkkk freq_hz / qsy_candidates
                    // params; the inner falls back to a single dial of `target`.
                    None,
                    None,
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

// ---------------------------------------------------------------------------
// Write port (phase 3.4) — GATED Agent config/state writes.
//
// EVERY method VALIDATES the agent-supplied input FIRST (a malformed value is
// rejected as `WritePortError::Invalid` via the `?`-on-`ValidationError` `From`
// impl, BEFORE the gate is reached, so the armed grant is never consumed by a
// bad input), THEN runs the mutation through
// `guarded_egress(.., EgressAuthority::Agent, ..)`. A disarmed / expired /
// tainted / poisoned session yields `EgressDenied` → `WritePortError::Denied`
// and NOTHING is written; an authorized op that then fails operationally yields
// `WritePortError::Failed`. The GUI Tauri commands are UNCHANGED (Operator is
// unconditionally allowed and never reaches this path).
//
// Audit: each op reuses `write_audit_sink` (same shape as `egress_audit_sink`,
// labeled "[write]" so the operator can tell a config/state write from a
// transmit/connect egress in the session log).
// ---------------------------------------------------------------------------

/// Build the audit sink for one gated WRITE: writes an operator-visible
/// session-log line (Warn on denial, Info on allow) and a structured tracing
/// record under `target: "tuxlink::write_gate"`. Mirrors [`egress_audit_sink`]
/// but labels the line `[write]` so a config/state mutation is distinguishable
/// from a transmit/connect egress in the operator's log. Captures only `Clone`
/// handles → `Send + Sync`.
fn write_audit_sink(app: AppHandle) -> impl Fn(EgressAudit<'_>) + Send + Sync {
    use crate::winlink_backend::{LogLevel, LogSource};
    move |a: EgressAudit<'_>| {
        let log = app.state::<Arc<crate::session_log::SessionLogState>>();
        if a.allowed {
            let msg = format!("[write] {} authorized for Agent", a.op);
            tracing::info!(
                target: "tuxlink::write_gate",
                op = a.op,
                authority = ?a.authority,
                allowed = true,
                "agent write authorized"
            );
            log.append_operator_line(LogLevel::Info, LogSource::Backend, msg);
        } else {
            let reason = a.reason.as_deref().unwrap_or("denied");
            let msg = format!("[write] {} DENIED for Agent: {reason}", a.op);
            tracing::warn!(
                target: "tuxlink::write_gate",
                op = a.op,
                authority = ?a.authority,
                allowed = false,
                reason = reason,
                "agent write denied"
            );
            log.append_operator_line(LogLevel::Warn, LogSource::Backend, msg);
        }
    }
}

/// [`WritePort`] adapter that validates input first, then gates each Agent
/// config/state write through the shared [`EgressGuard`] before persisting via
/// the existing command-layer logic.
pub struct MonolithWritePort {
    app: AppHandle,
    guard: Arc<EgressGuard>,
}

impl MonolithWritePort {
    pub fn new(app: AppHandle, guard: Arc<EgressGuard>) -> Self {
        Self { app, guard }
    }
}

#[async_trait]
impl WritePort for MonolithWritePort {
    async fn set_ardop(&self, dto: ArdopWriteDto) -> Result<(), WritePortError> {
        // VALIDATE BEFORE GATE: an out-of-range drive level is `Invalid` even
        // when disarmed; `?` returns before `guarded_egress` is reached.
        validate_drive_level(dto.drive_level)?;
        let audit = write_audit_sink(self.app.clone());
        let drive_level = dto.drive_level;
        guarded_egress(&self.guard, EgressAuthority::Agent, "set_ardop", &audit, || async move {
            // Read the current ArdopUiConfig, mutate ONLY drive_level, persist.
            // (modem_commands.rs:107 config_get_ardop / :117 config_set_ardop —
            // the latter read-modify-writes the whole config atomically.) The
            // agent may not touch any other ARDOP field.
            let mut cfg = crate::modem_commands::config_get_ardop();
            cfg.drive_level = Some(drive_level);
            crate::modem_commands::config_set_ardop(cfg).map_err(WritePortError::Failed)
        })
        .await
        .map_err(|d| WritePortError::Denied(d.to_string()))?
    }

    async fn set_vara(&self, dto: VaraWriteDto) -> Result<(), WritePortError> {
        // VALIDATE BEFORE GATE: only 500/2300/2750 Hz are accepted.
        validate_vara_bandwidth(dto.bandwidth_hz)?;
        // Defense-in-depth: confirm the value maps to a canonical Bandwidth
        // variant before the gate (validate_vara_bandwidth + bandwidth_from_hz
        // share the same valid set; a divergence here would be Invalid).
        if crate::winlink::modem::vara::commands::bandwidth_from_hz(dto.bandwidth_hz).is_none() {
            return Err(WritePortError::Invalid(format!(
                "vara bandwidth {} Hz is not a supported width",
                dto.bandwidth_hz
            )));
        }
        let audit = write_audit_sink(self.app.clone());
        let bandwidth_hz = dto.bandwidth_hz;
        guarded_egress(&self.guard, EgressAuthority::Agent, "set_vara", &audit, || async move {
            // Read the current VaraUiConfig, mutate ONLY bandwidth_hz, persist
            // via the same read-modify-write-atomic path the ARDOP setter uses
            // (vara/commands.rs:983 config_get_vara / :993 config_set_vara).
            let mut cfg = crate::winlink::modem::vara::commands::config_get_vara();
            cfg.bandwidth_hz = Some(bandwidth_hz);
            crate::winlink::modem::vara::commands::config_set_vara(cfg)
                .map_err(WritePortError::Failed)
        })
        .await
        .map_err(|d| WritePortError::Denied(d.to_string()))?
    }

    async fn set_packet(&self, dto: PacketWriteDto) -> Result<(), WritePortError> {
        // VALIDATE BEFORE GATE (FIX 4): a malformed payload is `Invalid`
        // regardless of arm state, so an unarmed caller gets `Invalid` (not
        // `Denied`) and an armed session never audits a write that should never
        // have reached the gate.
        //   * SSID range 0..=15 (AX.25 SSID, mirrors config.rs:699
        //     PacketSsidOutOfRange). The downstream `cfg.validate()` also
        //     enforces this, but post-gate; check it here so it short-circuits
        //     before the gate.
        if dto.ssid > 15 {
            return Err(WritePortError::Invalid(format!(
                "packet ssid {} out of the 0..=15 AX.25 range",
                dto.ssid
            )));
        }
        // FIX 3: the agent surface can only set TCP/IP packet parameters
        // (host/port/ssid/txdelay). Read the CURRENT link kind: if the operator
        // has configured a non-TCP link (Serial / Bluetooth / UvproNative /
        // Managed Dire Wolf, plus its audio + PTT), REJECT — the agent must not
        // silently switch the link type and discard that configuration. Only an
        // already-TCP link (or no link yet) may be (re)pointed by the agent.
        // This read is side-effect-free, so it belongs pre-gate alongside the
        // SSID check.
        let mut current = crate::ui_commands::packet_config_get()
            .await
            .map_err(|e| WritePortError::Failed(format!("{e:?}")))?;
        match current.link_kind.as_deref() {
            Some("Tcp") | None => {}
            Some(other) => {
                return Err(WritePortError::Invalid(format!(
                    "packet link is not TCP (currently {other}); agent cannot change link type"
                )))
            }
        }
        // Map the narrow agent DTO onto the monolith PacketConfigDto: override
        // ONLY ssid/tcp_host/tcp_port/txdelay, and pin link_kind = "Tcp" so the
        // host/port apply (an absent link_kind would PRESERVE the prior link,
        // leaving the agent-supplied host/port inert). `current` already carries
        // the operator's other packet params (paclen, timing, etc.).
        let audit = write_audit_sink(self.app.clone());
        let app = self.app.clone();
        let PacketWriteDto {
            ssid,
            tcp_host,
            tcp_port,
            txdelay_ms,
        } = dto;
        guarded_egress(&self.guard, EgressAuthority::Agent, "set_packet", &audit, || async move {
            current.ssid = ssid;
            current.link_kind = Some("Tcp".to_string());
            current.tcp_host = Some(tcp_host);
            current.tcp_port = Some(tcp_port);
            // txdelay is a u8 on the monolith DTO; clamp the agent's u32 ms.
            current.txdelay = u8::try_from(txdelay_ms).unwrap_or(u8::MAX);
            crate::ui_commands::packet_config_set(
                app.clone(),
                app.state::<crate::app_backend::BackendState>(),
                current,
            )
            .await
            .map_err(|e| WritePortError::Failed(format!("{e:?}")))
        })
        .await
        .map_err(|d| WritePortError::Denied(d.to_string()))?
    }

    async fn set_grid(&self, grid: String) -> Result<(), WritePortError> {
        // VALIDATE BEFORE GATE (FIX 4): a malformed Maidenhead locator is
        // `Invalid` regardless of arm state. `config_set_grid_impl` already
        // rejects a bad grid (UiError::Rejected) but only AFTER the gate; check
        // it here (reusing the same validator) so a bad locator never reaches
        // the gate or gets audited as an authorized write.
        if let Some(reason) = crate::ui_commands::validate_grid_input(&grid) {
            return Err(WritePortError::Invalid(reason.to_string()));
        }
        let audit = write_audit_sink(self.app.clone());
        let app = self.app.clone();
        guarded_egress(&self.guard, EgressAuthority::Agent, "set_grid", &audit, || async move {
            // ui_commands.rs:6323 config_set_grid_impl (validates via
            // validate_grid_input → UiError::Rejected on a bad locator). Resolve
            // the arbiter + backend from managed state.
            let arbiter = app
                .state::<Arc<crate::position::PositionArbiter>>()
                .inner()
                .clone();
            let backend = app.state::<crate::app_backend::BackendState>().current();
            crate::ui_commands::config_set_grid_impl(grid, arbiter, backend)
                .await
                .map_err(|e| WritePortError::Failed(format!("{e:?}")))
        })
        .await
        .map_err(|d| WritePortError::Denied(d.to_string()))?
    }

    async fn set_position_source(&self, source: String) -> Result<(), WritePortError> {
        // VALIDATE BEFORE GATE (FIX 4): "Gps" is the only accepted source
        // (position_set_source_impl rejects anything else with UiError::Rejected,
        // but only after the gate). Reject an unknown source as `Invalid` here so
        // it never reaches the gate or audits an authorized write.
        if source != "Gps" {
            return Err(WritePortError::Invalid(format!(
                "unsupported position source '{source}' (only \"Gps\" is accepted)"
            )));
        }
        let audit = write_audit_sink(self.app.clone());
        let app = self.app.clone();
        guarded_egress(
            &self.guard,
            EgressAuthority::Agent,
            "set_position_source",
            &audit,
            || async move {
                // ui_commands.rs:6398 position_set_source_impl (rejects any
                // source other than "Gps" with UiError::Rejected).
                let arbiter = app
                    .state::<Arc<crate::position::PositionArbiter>>()
                    .inner()
                    .clone();
                let backend = app.state::<crate::app_backend::BackendState>().current();
                crate::ui_commands::position_set_source_impl(source, arbiter, backend)
                    .await
                    .map_err(|e| WritePortError::Failed(format!("{e:?}")))
            },
        )
        .await
        .map_err(|d| WritePortError::Denied(d.to_string()))?
    }

    async fn set_privacy(
        &self,
        gps_state: String,
        precision: String,
    ) -> Result<(), WritePortError> {
        use crate::config::{GpsState, PositionPrecision};
        // VALIDATE BEFORE GATE: parse the two agent strings into the typed
        // monolith enums; an unknown value is `Invalid` regardless of arm state.
        let gps_state = match gps_state.as_str() {
            "Off" => GpsState::Off,
            "LocalUiOnly" => GpsState::LocalUiOnly,
            "BroadcastAtPrecision" => GpsState::BroadcastAtPrecision,
            other => {
                return Err(WritePortError::Invalid(format!(
                    "unknown gps_state '{other}' (expected Off | LocalUiOnly | BroadcastAtPrecision)"
                )))
            }
        };
        let precision = match precision.as_str() {
            "FourCharGrid" => PositionPrecision::FourCharGrid,
            "SixCharGrid" => PositionPrecision::SixCharGrid,
            other => {
                return Err(WritePortError::Invalid(format!(
                    "unknown precision '{other}' (expected FourCharGrid | SixCharGrid)"
                )))
            }
        };
        let audit = write_audit_sink(self.app.clone());
        let app = self.app.clone();
        guarded_egress(&self.guard, EgressAuthority::Agent, "set_privacy", &audit, || async move {
            // Replicate config_set_privacy's body (ui_commands.rs:6568): read →
            // set both privacy fields → write atomically → sync the arbiter's
            // precision → refresh the live backend. The command itself takes
            // Tauri State extractors; call its inner logic against resolved
            // managed state so the agent path mirrors the GUI exactly.
            let arbiter = app
                .state::<Arc<crate::position::PositionArbiter>>()
                .inner()
                .clone();
            let mut cfg = crate::config::read_config()
                .map_err(|e| WritePortError::Failed(format!("{e:?}")))?;
            cfg.privacy.gps_state = gps_state;
            cfg.privacy.position_precision = precision;
            crate::config::write_config_atomic(&cfg)
                .map_err(|e| WritePortError::Failed(format!("{e:?}")))?;
            arbiter.set_precision(precision);
            if let Some(backend) = app.state::<crate::app_backend::BackendState>().current() {
                backend.set_config(cfg);
            }
            Ok::<(), WritePortError>(())
        })
        .await
        .map_err(|d| WritePortError::Denied(d.to_string()))?
    }

    async fn set_packet_listen(&self, enabled: bool) -> Result<(), WritePortError> {
        let audit = write_audit_sink(self.app.clone());
        guarded_egress(
            &self.guard,
            EgressAuthority::Agent,
            "set_packet_listen",
            &audit,
            || async move {
                // ui_commands.rs:4670 packet_set_listen(bool).
                crate::ui_commands::packet_set_listen(enabled)
                    .await
                    .map_err(|e| WritePortError::Failed(format!("{e:?}")))
            },
        )
        .await
        .map_err(|d| WritePortError::Denied(d.to_string()))?
    }

    async fn mailbox_move(
        &self,
        from: String,
        to: String,
        id: String,
    ) -> Result<(), WritePortError> {
        // VALIDATE BEFORE GATE (FIX 4): both endpoints must be known folder
        // refs. mailbox_move validates them via parse_folder_ref but only after
        // the gate; reject an unknown from/to as `Invalid` here so a malformed
        // move never reaches the gate or audits an authorized write.
        crate::ui_commands::parse_folder_ref(&from)
            .map_err(|e| WritePortError::Invalid(format!("invalid 'from' folder: {e:?}")))?;
        crate::ui_commands::parse_folder_ref(&to)
            .map_err(|e| WritePortError::Invalid(format!("invalid 'to' folder: {e:?}")))?;
        let audit = write_audit_sink(self.app.clone());
        let app = self.app.clone();
        guarded_egress(&self.guard, EgressAuthority::Agent, "mailbox_move", &audit, || async move {
            // ui_commands.rs:1426 mailbox_move (validates folders via
            // parse_folder_ref). Resolve BackendState from the AppHandle.
            crate::ui_commands::mailbox_move(
                from,
                to,
                id,
                app.state::<crate::app_backend::BackendState>(),
            )
            .await
            .map_err(|e| WritePortError::Failed(format!("{e:?}")))
        })
        .await
        .map_err(|d| WritePortError::Denied(d.to_string()))?
    }

    async fn attachment_save(
        &self,
        folder: String,
        id: String,
        filename: String,
        dest: String,
    ) -> Result<String, WritePortError> {
        use crate::native_mailbox::FolderRef;
        use crate::winlink_backend::MessageId;
        // The agent attachment-save target is a fixed, sandboxed base —
        // <app_data>/agent-attachments — NOT the native save dialog the GUI
        // command uses. Compute + create the base, then VALIDATE the dest
        // against it BEFORE the gate so a traversal/absolute/escaping dest is
        // rejected as `Invalid` regardless of arm state. The actual mailbox
        // read + FS write happen INSIDE the gated op.
        let base = self
            .app
            .path()
            .app_data_dir()
            .map_err(|e| WritePortError::Failed(format!("{e:?}")))?
            .join("agent-attachments");
        std::fs::create_dir_all(&base)
            .map_err(|e| WritePortError::Failed(format!("create agent-attachments dir: {e}")))?;
        // VALIDATE BEFORE GATE: canonicalize + contain the requested dest under
        // the base; `?`-on-`ValidationError` returns `Invalid` before the gate.
        let path = validate_attachment_dest(&base, &dest)?;

        let audit = write_audit_sink(self.app.clone());
        let app = self.app.clone();
        guarded_egress(
            &self.guard,
            EgressAuthority::Agent,
            "attachment_save",
            &audit,
            || async move {
                // Read the message + extract the attachment bytes by filename —
                // the same read/extract path the GUI command uses
                // (ui_commands.rs:1350 message_attachment_save), minus the native
                // dialog. The gated FS write lands at the pre-validated `path`.
                let backend = app
                    .state::<crate::app_backend::BackendState>()
                    .current()
                    .ok_or_else(|| WritePortError::Failed("backend offline".to_string()))?;
                let parsed = crate::ui_commands::parse_folder_ref(&folder)
                    .map_err(|e| WritePortError::Failed(format!("{e:?}")))?;
                let mid = MessageId::new(&id);
                let body = match &parsed {
                    FolderRef::System(f) => backend
                        .read_message_in(*f, &mid)
                        .await
                        .map_err(|e| WritePortError::Failed(format!("{e:?}")))?,
                    FolderRef::User(slug) => backend
                        .read_user_message(slug, &mid)
                        .await
                        .map_err(|e| WritePortError::Failed(format!("{e:?}")))?,
                };
                let msg = mail_parser::MessageParser::new()
                    .parse(body.raw_rfc5322.as_slice())
                    .ok_or_else(|| {
                        WritePortError::Failed(format!("could not parse message {id}"))
                    })?;
                let bytes = crate::ui_commands::extract_attachment_bytes(
                    &msg,
                    body.raw_rfc5322.as_slice(),
                    &filename,
                )
                .ok_or_else(|| {
                    WritePortError::Failed(format!("attachment '{filename}' not in message {id}"))
                })?;
                // FS write off the async runtime (spawn_blocking) at the
                // pre-validated, base-contained path. The prevalidated PathBuf
                // (validate_attachment_dest) proves the PARENT canonicalizes
                // under the base, but `fs::write` follows the FINAL component
                // through a symlink — a leaf symlink planted at `path` (or a
                // parent swapped after validation) would redirect the write
                // outside the sandbox. Two defenses, applied AT WRITE TIME:
                //   1. symlink_metadata refusal: if the final path already
                //      exists AND is a symlink, reject before opening.
                //   2. O_NOFOLLOW open: open with O_CREAT|O_WRONLY|O_TRUNC and
                //      O_NOFOLLOW so the kernel refuses to follow a leaf symlink
                //      at the final component (closes the validate→write TOCTOU).
                // (Mirrors the tiles/cache.rs leaf-symlink guard.)
                let write_path = path.clone();
                tokio::task::spawn_blocking(move || -> std::io::Result<()> {
                    use std::io::Write;
                    use std::os::unix::fs::OpenOptionsExt;
                    let final_is_symlink = std::fs::symlink_metadata(&write_path)
                        .map(|m| m.file_type().is_symlink())
                        .unwrap_or(false);
                    if final_is_symlink {
                        return Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied));
                    }
                    let mut file = std::fs::OpenOptions::new()
                        .write(true)
                        .create(true)
                        .truncate(true)
                        .custom_flags(libc::O_NOFOLLOW)
                        .open(&write_path)?;
                    file.write_all(&bytes)?;
                    Ok(())
                })
                .await
                .map_err(|e| WritePortError::Failed(format!("write task failed: {e}")))?
                .map_err(|e| WritePortError::Failed(format!("write attachment: {e}")))?;
                Ok::<String, WritePortError>(path.to_string_lossy().into_owned())
            },
        )
        .await
        .map_err(|d| WritePortError::Denied(d.to_string()))?
    }
}

// ---------------------------------------------------------------------------
// Compose port (phase 3.4) — UNGATED local-outbox staging.
//
// EVERY method VALIDATES the agent-supplied input (addresses for CR/LF header
// injection + control chars + length; subject/body length where applicable) and
// then calls the existing compose/send-to-outbox command DIRECTLY — NO gate,
// because nothing is transmitted: a staged outbox draft only leaves the box on a
// later GATED connect. So a compose succeeds without an arm and can never be
// `Denied`. Each method returns the staged MID string the underlying command
// yields.
// ---------------------------------------------------------------------------

/// [`ComposePort`] adapter over the message/form/catalog/GRIB outbox-staging
/// commands. Validates agent input, then stages a LOCAL draft (no egress gate).
pub struct MonolithComposePort {
    app: AppHandle,
}

impl MonolithComposePort {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }

    /// Validate every recipient (`to` + `cc`) for header injection / control
    /// chars / length. Returns `Invalid` on the first offender.
    fn validate_recipients(to: &[String], cc: &[String]) -> Result<(), WritePortError> {
        for addr in to.iter().chain(cc.iter()) {
            validate_address(addr)?;
        }
        Ok(())
    }
}

#[async_trait]
impl ComposePort for MonolithComposePort {
    async fn message_send(&self, dto: ComposeDraftDto) -> Result<String, WritePortError> {
        // VALIDATE recipients + subject + body BEFORE staging.
        Self::validate_recipients(&dto.to, &dto.cc)?;
        validate_subject(&dto.subject)?;
        validate_body(&dto.body)?;
        // Map onto the monolith OutboundDraftDto (no attachments from the agent
        // surface) and stage via ui_commands.rs:1837 message_send (UNGATED —
        // queues in the outbox; transmission is a later gated connect).
        let draft = crate::ui_commands::OutboundDraftDto {
            to: dto.to,
            cc: dto.cc,
            subject: dto.subject,
            body: dto.body,
            attachments: Vec::new(),
        };
        crate::ui_commands::message_send(
            draft,
            self.app.state::<crate::app_backend::BackendState>(),
        )
        .await
        .map_err(|e| WritePortError::Failed(format!("{e:?}")))
    }

    async fn send_form(&self, dto: SendFormDto) -> Result<String, WritePortError> {
        // VALIDATE recipients + the sender callsign + grid for CR/LF + control
        // chars (the addresses/identity fields are a header-injection surface).
        Self::validate_recipients(&dto.to, &dto.cc)?;
        validate_address(&dto.senders_callsign)?;
        validate_address(&dto.grid_square)?;
        // send_form (ui_commands.rs:1893) takes a HashMap; the DTO carries a
        // BTreeMap. Convert.
        let field_values: std::collections::HashMap<String, String> =
            dto.field_values.into_iter().collect();
        // FIX 2: the form SUBJECT is rendered from `form.subject_template` with
        // the agent's field_values (ui_commands::send_form does exactly
        // `render_body_template(form.subject_template, &field_values)`). The
        // renderer drops XML-1.0-illegal control chars but DELIBERATELY keeps
        // 0x9/0x0A/0x0D (is_xml10_legal), so a CR/LF inside a subject-bound field
        // would survive into the rendered subject and split the RFC5322 header
        // block — the to/cc/identity checks above never see it. Defense: replicate
        // the exact subject render the command will do, then run validate_subject
        // on the RESULT before staging. This validates only the SUBJECT-bound
        // fields (whichever the template references), so a legitimately multi-line
        // BODY field is not over-rejected. Look up the form first so an unknown
        // form_id still surfaces "unknown form" from send_form (we mirror its
        // find_form lookup but tolerate a miss here and let send_form report it).
        if let Some(form) = crate::forms::catalog::find_form(&dto.form_id) {
            let rendered_subject = crate::forms::serialize::render_body_template(
                form.subject_template,
                &field_values,
            );
            validate_subject(&rendered_subject)?;
        }
        crate::ui_commands::send_form(
            dto.form_id,
            field_values,
            dto.to,
            dto.cc,
            dto.senders_callsign,
            dto.grid_square,
            self.app.state::<crate::app_backend::BackendState>(),
        )
        .await
        .map_err(|e| WritePortError::Failed(format!("{e:?}")))
    }

    async fn catalog_send_inquiry(
        &self,
        item_ids: Vec<String>,
    ) -> Result<String, WritePortError> {
        // VALIDATE each filename for control chars + length (reuse the address
        // validator's control/CRLF/len checks; catalog filenames carry no `@`
        // semantics but must not inject CRLF or control bytes into the inquiry
        // body). The composer (catalog::commands::catalog_send_inquiry) does its
        // own body-composition validation.
        for fname in &item_ids {
            validate_address(fname)?;
        }
        crate::catalog::commands::catalog_send_inquiry(
            item_ids,
            self.app.state::<crate::app_backend::BackendState>(),
        )
        .await
        .map_err(|e| WritePortError::Failed(format!("{e:?}")))
    }

    async fn grib_send_request(&self, dto: GribRequestDto) -> Result<String, WritePortError> {
        use crate::grib::composer::{
            ForecastTime, GribDirection, GribMode, GribParameter, GribRequest, Latitude, Longitude,
        };
        // VALIDATE the subject (CR/LF header injection + length) BEFORE staging.
        validate_subject(&dto.subject)?;
        // Map the coarse mode string onto GribMode; unknown → Invalid.
        let mode = match dto.mode.to_ascii_lowercase().as_str() {
            "send" => GribMode::Send,
            "sub" => GribMode::Sub,
            other => {
                return Err(WritePortError::Invalid(format!(
                    "unknown grib mode '{other}' (expected send | sub)"
                )))
            }
        };
        // Validate the agent-supplied center coordinates are on the globe.
        if !(-90.0..=90.0).contains(&dto.lat) {
            return Err(WritePortError::Invalid(format!(
                "latitude {} out of range [-90, 90]",
                dto.lat
            )));
        }
        if !(-180.0..=180.0).contains(&dto.lon) {
            return Err(WritePortError::Invalid(format!(
                "longitude {} out of range [-180, 180]",
                dto.lon
            )));
        }
        // The agent surface supplies a single CENTER point; Saildocs needs a
        // bounding box. Build a 10°-wide box around the center, clamped to the
        // globe, and split each bound into whole-degree magnitude + hemisphere.
        // Defaults (grid 2x2, empty times/params) defer to Saildocs' own
        // defaults, matching the composer's documented behavior.
        const HALF: f64 = 5.0;
        let lat0_v = (dto.lat - HALF).clamp(-90.0, 90.0);
        let lat1_v = (dto.lat + HALF).clamp(-90.0, 90.0);
        let lon0_v = (dto.lon - HALF).clamp(-180.0, 180.0);
        let lon1_v = (dto.lon + HALF).clamp(-180.0, 180.0);
        let to_lat = |v: f64| -> Latitude {
            let dir = if v >= 0.0 {
                GribDirection::N
            } else {
                GribDirection::S
            };
            Latitude {
                degrees: v.abs().round() as u8,
                dir,
            }
        };
        let to_lon = |v: f64| -> Longitude {
            let dir = if v >= 0.0 {
                GribDirection::E
            } else {
                GribDirection::W
            };
            Longitude {
                degrees: v.abs().round() as u16,
                dir,
            }
        };
        let request = GribRequest {
            mode,
            lat0: to_lat(lat0_v),
            lat1: to_lat(lat1_v),
            lon0: to_lon(lon0_v),
            lon1: to_lon(lon1_v),
            grid: (2, 2),
            times: Vec::<ForecastTime>::new(),
            params: Vec::<GribParameter>::new(),
            sub_days: None,
            sub_time: None,
            subject: dto.subject,
        };
        crate::grib::commands::grib_send_request(
            request,
            self.app.state::<crate::app_backend::BackendState>(),
        )
        .await
        .map_err(|e| WritePortError::Failed(format!("{e:?}")))
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
