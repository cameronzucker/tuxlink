//! Canned mock port impls for the tier-2 testserver (phase 3.2).
//!
//! These return deterministic, recognizable data so the tier-2 round-trip can:
//! - call a read tool and verify the DTO shape made it across the UDS, and
//! - read [`SEED_MSG`] (from + subject below) then assert taint flipped.
//!
//! The MailboxPort mock seeds ONE message with a recognizable
//! `from`/`subject`; the ConfigPort mock returns a 4-char grid (`"CN87"`).

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_trait::async_trait;

use tuxlink_mcp_core::ports::{
    AbortPort, ArdopConfigDto, ArdopWriteDto, AttachmentMetaDto, AudioDevicesDto, BackendStatusDto,
    BluetoothDeviceDto, CatalogEntryDto, ChannelReliabilityDto, ComposeDraftDto, ComposePort,
    ConfigPort, ConfigViewDto, DevicePort, DocsHitDto, EgressPort, EgressPortError, FolderDto,
    GatewayAntennaDto, GatewayDto, GribRequestDto, LogLineDto, LogPort, MailboxPort,
    MessageMetaDto, ModemStatusDto, PacketConfigDto, PacketWriteDto, ParsedMessageDto,
    PathPredictionDto, PlatformInfoDto, PortError, PositionStatusDto, PredictRequestDto,
    PredictionPort, PrinterDto, ProvisionPort, QsyCandidateDto, RigConfigDto, RigStatusDto,
    SearchPort, SearchQueryDto, SearchResultsDto, SendFormDto, SerialDeviceDto, SessionIntentDto,
    SolarSnapshotDto, StationFilterDto, StationListDto, StationModeDto, StationPort, StatusPort,
    VaraCheckpointDto, VaraConfigDto, VaraInstallStatusDto, VaraInstallSummaryDto, VaraProbeDto,
    VaraStatusDto, VaraWriteDto, WritePort, WritePortError,
};
use tuxlink_mcp_core::validate::{
    validate_address, validate_attachment_dest, validate_body, validate_drive_level,
    validate_subject, validate_vara_bandwidth,
};
use tuxlink_security::{guarded_egress, EgressAudit, EgressAuthority, EgressGuard};

/// Recognizable seeded message — tier-2 reads this and verifies the round-trip.
pub const SEED_MSG_ID: &str = "MSG001";
pub const SEED_MSG_FROM: &str = "W1AW";
pub const SEED_MSG_SUBJECT: &str = "ARES net check-in";
/// 4-char grid the ConfigPort mock reports.
pub const SEED_GRID: &str = "CN87";
/// Station-intel fixtures: the gateway `find_stations` seeds + the `tx_grid`
/// (4-char) `predict_path` returns.
pub const SEED_GW_CALLSIGN: &str = "W1AW";
pub const SEED_GW_GRID: &str = "FN31";
pub const SEED_GW_FREQ_KHZ: f64 = 7104.0;
pub const SEED_TX_GRID: &str = "CN87";

pub struct MockStatus;

#[async_trait]
impl StatusPort for MockStatus {
    async fn backend_status(&self) -> Result<BackendStatusDto, PortError> {
        Ok(BackendStatusDto {
            connected: true,
            transport: "telnet".into(),
            state: "idle".into(),
        })
    }
    async fn modem_status(&self) -> Result<ModemStatusDto, PortError> {
        Ok(ModemStatusDto {
            kind: "idle".into(),
            connected: false,
            state: "idle".into(),
            running: vec![],
            selected: None,
            conflict: false,
        })
    }
    async fn vara_status(&self) -> Result<VaraStatusDto, PortError> {
        Ok(VaraStatusDto {
            connected: false,
            bandwidth: 2300,
            state: "idle".into(),
            reachable: Some(false),
        })
    }
    async fn vara_probe(&self) -> Result<VaraProbeDto, PortError> {
        Ok(VaraProbeDto {
            classification: "down".into(),
            banner: None,
        })
    }
    async fn position_status(&self) -> Result<PositionStatusDto, PortError> {
        Ok(PositionStatusDto {
            has_fix: true,
            grid: SEED_GRID.into(),
            source: "gps".into(),
        })
    }
    async fn platform_info(&self) -> Result<PlatformInfoDto, PortError> {
        Ok(PlatformInfoDto {
            os: "linux".into(),
            arch: "aarch64".into(),
            app_version: "testserver".into(),
        })
    }
    async fn wizard_completed(&self) -> Result<bool, PortError> {
        Ok(true)
    }
    async fn p2p_peer_password_status(&self, _callsign: &str) -> Result<bool, PortError> {
        Ok(true)
    }
    async fn rig_status(&self) -> Result<RigStatusDto, PortError> {
        Ok(RigStatusDto {
            vfo_hz: Some(7_104_000),
            mode: Some("PKTUSB".into()),
            ptt: Some(false),
            configured: true,
        })
    }
}

pub struct MockMailbox;

#[async_trait]
impl MailboxPort for MockMailbox {
    async fn list(&self, _folder: &str) -> Result<Vec<MessageMetaDto>, PortError> {
        Ok(vec![MessageMetaDto {
            id: SEED_MSG_ID.into(),
            subject: SEED_MSG_SUBJECT.into(),
            from: SEED_MSG_FROM.into(),
            to: "TEST".into(),
            date: "2026-06-26T00:00:00Z".into(),
            unread: true,
            has_attachments: false,
        }])
    }
    async fn read(&self, _folder: &str, id: &str) -> Result<ParsedMessageDto, PortError> {
        Ok(ParsedMessageDto {
            id: id.into(),
            subject: SEED_MSG_SUBJECT.into(),
            from: SEED_MSG_FROM.into(),
            to: "TEST".into(),
            cc: String::new(),
            date: "2026-06-26T00:00:00Z".into(),
            body: "Check in for the evening ARES net.".into(),
            attachments: vec![AttachmentMetaDto {
                filename: "roster.txt".into(),
                size: 42,
            }],
            has_form: false,
        })
    }
    async fn folders(&self) -> Result<Vec<FolderDto>, PortError> {
        Ok(vec![FolderDto {
            name: "Inbox".into(),
            count: 1,
        }])
    }
}

pub struct MockSearch;

#[async_trait]
impl SearchPort for MockSearch {
    async fn messages(&self, _query: SearchQueryDto) -> Result<SearchResultsDto, PortError> {
        Ok(SearchResultsDto {
            items: vec![MessageMetaDto {
                id: SEED_MSG_ID.into(),
                subject: SEED_MSG_SUBJECT.into(),
                from: SEED_MSG_FROM.into(),
                to: "TEST".into(),
                date: "2026-06-26T00:00:00Z".into(),
                unread: true,
                has_attachments: false,
            }],
            total: 1,
        })
    }
    async fn docs(&self, _query: &str) -> Result<Vec<DocsHitDto>, PortError> {
        Ok(vec![DocsHitDto {
            title: "Getting started".into(),
            path: "user-guide/start.md".into(),
            snippet: "Connect to a CMS gateway.".into(),
        }])
    }
    async fn catalog(&self) -> Result<Vec<CatalogEntryDto>, PortError> {
        Ok(vec![CatalogEntryDto {
            id: "ICS-213".into(),
            title: "General Message".into(),
            category: "ICS".into(),
        }])
    }
}

pub struct MockConfig;

#[async_trait]
impl ConfigPort for MockConfig {
    async fn read(&self) -> Result<ConfigViewDto, PortError> {
        Ok(ConfigViewDto {
            connect_to_cms: true,
            transport: "telnet".into(),
            host: "cms-z.winlink.org".into(),
            callsign: "TEST".into(),
            grid: SEED_GRID.into(),
        })
    }
    async fn ardop(&self) -> Result<ArdopConfigDto, PortError> {
        Ok(ArdopConfigDto {
            host: "127.0.0.1".into(),
            port: 8515,
            drive_level: 80,
            bandwidth: 2000,
        })
    }
    async fn vara(&self) -> Result<VaraConfigDto, PortError> {
        Ok(VaraConfigDto {
            host: "127.0.0.1".into(),
            port: 8300,
            bandwidth: 2300,
            drive_level: 80,
        })
    }
    async fn packet(&self) -> Result<PacketConfigDto, PortError> {
        Ok(PacketConfigDto {
            kiss_host: "127.0.0.1".into(),
            kiss_port: 8001,
            baud: 1200,
            tx_delay: 30,
        })
    }
    async fn rig(&self) -> Result<RigConfigDto, PortError> {
        Ok(RigConfigDto {
            rig_hamlib_model: Some(1035),
            rigctld_host: "127.0.0.1".into(),
            rigctld_port: 4534,
            rigctld_binary: "rigctld".into(),
            close_serial_sequencing: false,
            live_vfo_poll: false,
            qsy_on_fail: false,
            cat_serial_path: Some("/dev/ttyUSB0".into()),
            cat_baud: 38400,
        })
    }
}

pub struct MockDevice;

#[async_trait]
impl DevicePort for MockDevice {
    async fn serial(&self) -> Result<Vec<SerialDeviceDto>, PortError> {
        Ok(vec![SerialDeviceDto {
            path: "/dev/ttyUSB0".into(),
            description: "USB Serial".into(),
        }])
    }
    async fn bluetooth(&self) -> Result<Vec<BluetoothDeviceDto>, PortError> {
        Ok(vec![BluetoothDeviceDto {
            name: "TNC3".into(),
            mac: "AA:BB:**:**:**:01".into(),
        }])
    }
    async fn audio(&self) -> Result<AudioDevicesDto, PortError> {
        Ok(AudioDevicesDto {
            capture: vec!["default".into()],
            playback: vec!["default".into()],
            cards: vec![],
        })
    }
    async fn printer_list(&self) -> Result<Vec<PrinterDto>, PortError> {
        Ok(vec![])
    }
    async fn print_document(&self, _printer: String, _filename: String) -> Result<(), PortError> {
        Ok(())
    }
    async fn export_report(&self, filename: String, _content: String) -> Result<String, PortError> {
        Ok(format!("/mock/reports/{filename}"))
    }
}

pub struct MockLog;

#[async_trait]
impl LogPort for MockLog {
    async fn snapshot(&self) -> Result<Vec<LogLineDto>, PortError> {
        Ok(vec![LogLineDto {
            timestamp: "2026-06-26T00:00:00Z".into(),
            level: "info".into(),
            message: "session started".into(),
        }])
    }
}

/// GATED egress mock for the tier-2 testserver (phase 3.3).
///
/// Holds the SAME [`EgressGuard`] the testserver built from the environment, so
/// `TUXLINK_TEST_ARM` / `TUXLINK_TEST_TAINT` directly affect whether a gated
/// egress is allowed. Every method runs through the REAL
/// [`guarded_egress`](tuxlink_security::guarded_egress) with
/// [`EgressAuthority::Agent`]; the gated op flips a shared `op_ran` flag and
/// stderr-logs the decision via the audit sink, so a tier-2 run demonstrates
/// the real gate end-to-end (denied → no transmit; armed+untainted → transmit).
pub struct MockEgress {
    guard: Arc<EgressGuard>,
    op_ran: Arc<AtomicBool>,
}

impl MockEgress {
    pub fn new(guard: Arc<EgressGuard>, op_ran: Arc<AtomicBool>) -> Self {
        Self { guard, op_ran }
    }

    async fn gated(&self, label: &str) -> Result<(), EgressPortError> {
        let op_ran = Arc::clone(&self.op_ran);
        let audit = |a: EgressAudit<'_>| {
            eprintln!(
                "egress-audit op={} allowed={} reason={:?}",
                a.op, a.allowed, a.reason
            );
        };
        guarded_egress(
            &self.guard,
            EgressAuthority::Agent,
            label,
            &audit,
            || async move {
                op_ran.store(true, Ordering::SeqCst);
            },
        )
        .await
        .map_err(|d| EgressPortError::Denied(d.to_string()))
    }
}

#[async_trait]
impl EgressPort for MockEgress {
    async fn cms_connect(&self) -> Result<(), EgressPortError> {
        self.gated("cms_connect").await
    }
    async fn verify_cms_connection(&self) -> Result<(), EgressPortError> {
        self.gated("verify_cms_connection").await
    }
    async fn rig_tune(&self, _freq_hz: u64) -> Result<(), EgressPortError> {
        self.gated("rig_tune").await
    }
    async fn ardop_connect(
        &self,
        _target: String,
        _freq_hz: Option<u64>,
        _qsy_candidates: Option<Vec<QsyCandidateDto>>,
    ) -> Result<(), EgressPortError> {
        self.gated("ardop_connect").await
    }
    async fn ardop_b2f_exchange(
        &self,
        _target: String,
        _intent: SessionIntentDto,
    ) -> Result<(), EgressPortError> {
        self.gated("ardop_b2f_exchange").await
    }
    async fn vara_b2f_exchange(
        &self,
        _target: String,
        _intent: SessionIntentDto,
        _freq_hz: Option<u64>,
        _qsy_candidates: Option<Vec<QsyCandidateDto>>,
    ) -> Result<(), EgressPortError> {
        self.gated("vara_b2f_exchange").await
    }
    async fn vara_open_session(&self, _intent: SessionIntentDto) -> Result<(), EgressPortError> {
        self.gated("vara_open_session").await
    }
    async fn packet_connect(
        &self,
        _call: String,
        _path: Vec<String>,
    ) -> Result<(), EgressPortError> {
        self.gated("packet_connect").await
    }
}

/// UNGATED abort mock. Flips a shared `aborted` flag; never gated.
pub struct MockAbort {
    aborted: Arc<AtomicBool>,
}

impl MockAbort {
    pub fn new(aborted: Arc<AtomicBool>) -> Self {
        Self { aborted }
    }
    fn mark(&self) {
        self.aborted.store(true, Ordering::SeqCst);
    }
}

#[async_trait]
impl AbortPort for MockAbort {
    async fn cms_abort(&self) -> Result<(), PortError> {
        self.mark();
        Ok(())
    }
    async fn ardop_disconnect(&self) -> Result<(), PortError> {
        self.mark();
        Ok(())
    }
    async fn vara_stop_session(&self) -> Result<(), PortError> {
        self.mark();
        Ok(())
    }
}

/// GATED write mock for the tier-2 testserver (phase 3.4).
///
/// Mirrors the tier-1 `MockWrite`: every method runs the relevant `validate.rs`
/// check FIRST (a bad value → [`WritePortError::Invalid`] without consuming the
/// armed grant), then gates the mutation through the REAL [`guarded_egress`]
/// against the SAME [`EgressGuard`] the testserver built from the environment,
/// flipping `op_ran` inside the gated op. `attachment_save` validates `dest`
/// against the injected `base` dir.
pub struct MockWrite {
    guard: Arc<EgressGuard>,
    op_ran: Arc<AtomicBool>,
    base: PathBuf,
}

impl MockWrite {
    pub fn new(guard: Arc<EgressGuard>, op_ran: Arc<AtomicBool>, base: PathBuf) -> Self {
        Self {
            guard,
            op_ran,
            base,
        }
    }

    async fn gated(&self, label: &str) -> Result<(), WritePortError> {
        let op_ran = Arc::clone(&self.op_ran);
        let audit = |a: EgressAudit<'_>| {
            eprintln!(
                "write-audit op={} allowed={} reason={:?}",
                a.op, a.allowed, a.reason
            );
        };
        guarded_egress(
            &self.guard,
            EgressAuthority::Agent,
            label,
            &audit,
            || async move {
                op_ran.store(true, Ordering::SeqCst);
            },
        )
        .await
        .map_err(|d| WritePortError::Denied(d.to_string()))
    }
}

#[async_trait]
impl WritePort for MockWrite {
    async fn set_ardop(&self, dto: ArdopWriteDto) -> Result<(), WritePortError> {
        validate_drive_level(dto.drive_level)?;
        self.gated("set_ardop").await
    }
    async fn set_vara(&self, dto: VaraWriteDto) -> Result<(), WritePortError> {
        validate_vara_bandwidth(dto.bandwidth_hz)?;
        self.gated("set_vara").await
    }
    async fn set_packet(&self, _dto: PacketWriteDto) -> Result<(), WritePortError> {
        self.gated("set_packet").await
    }
    async fn set_grid(&self, _grid: String) -> Result<(), WritePortError> {
        self.gated("set_grid").await
    }
    async fn set_position_source(&self, _source: String) -> Result<(), WritePortError> {
        self.gated("set_position_source").await
    }
    async fn set_privacy(
        &self,
        _gps_state: String,
        _precision: String,
    ) -> Result<(), WritePortError> {
        self.gated("set_privacy").await
    }
    async fn set_packet_listen(&self, _enabled: bool) -> Result<(), WritePortError> {
        self.gated("set_packet_listen").await
    }
    async fn mailbox_move(
        &self,
        _from: String,
        _to: String,
        _id: String,
    ) -> Result<(), WritePortError> {
        self.gated("mailbox_move").await
    }
    async fn attachment_save(
        &self,
        _folder: String,
        _id: String,
        _filename: String,
        dest: String,
    ) -> Result<String, WritePortError> {
        let path = validate_attachment_dest(&self.base, &dest)?;
        self.gated("attachment_save").await?;
        Ok(path.to_string_lossy().into_owned())
    }
}

/// UNGATED compose mock for the tier-2 testserver (phase 3.4).
///
/// Validates recipients/subject/body and, on success, flips a shared `staged`
/// flag and returns a canned MID. NEVER touches the guard and never taints, so a
/// compose succeeds without an arm and cannot be `Denied`.
pub struct MockCompose {
    staged: Arc<AtomicBool>,
}

impl MockCompose {
    pub fn new(staged: Arc<AtomicBool>) -> Self {
        Self { staged }
    }
    fn validate_recipients(to: &[String], cc: &[String]) -> Result<(), WritePortError> {
        for addr in to.iter().chain(cc.iter()) {
            validate_address(addr)?;
        }
        Ok(())
    }
    fn stage(&self) -> String {
        self.staged.store(true, Ordering::SeqCst);
        "STAGED-MID-0001".to_string()
    }
}

#[async_trait]
impl ComposePort for MockCompose {
    async fn message_send(&self, dto: ComposeDraftDto) -> Result<String, WritePortError> {
        Self::validate_recipients(&dto.to, &dto.cc)?;
        validate_subject(&dto.subject)?;
        validate_body(&dto.body)?;
        Ok(self.stage())
    }
    async fn send_form(&self, dto: SendFormDto) -> Result<String, WritePortError> {
        Self::validate_recipients(&dto.to, &dto.cc)?;
        Ok(self.stage())
    }
    async fn catalog_send_inquiry(&self, _item_ids: Vec<String>) -> Result<String, WritePortError> {
        Ok(self.stage())
    }
    async fn grib_send_request(&self, dto: GribRequestDto) -> Result<String, WritePortError> {
        validate_subject(&dto.subject)?;
        Ok(self.stage())
    }
}

/// Station-intelligence read mock for the tier-2 testserver (phase 3.2).
///
/// `find_stations` returns ONE recognizable gateway (W1AW / FN31 / VaraHf /
/// [7104.0] / Dipole). Read-only — never touches the guard (no taint, no gate),
/// mirroring the inert tier-1 read tools.
pub struct MockStation;

#[async_trait]
impl StationPort for MockStation {
    async fn find_stations(&self, _filter: StationFilterDto) -> Result<StationListDto, PortError> {
        Ok(StationListDto {
            gateways: vec![GatewayDto {
                mode: StationModeDto::VaraHf,
                channel: "7104.0 VARA HF".into(),
                callsign: SEED_GW_CALLSIGN.into(),
                grid: Some(SEED_GW_GRID.into()),
                frequencies_khz: vec![SEED_GW_FREQ_KHZ],
                antenna: Some(GatewayAntennaDto::Dipole),
                distance_km: None,
                distance_mi: None,
                bearing_deg: None,
            }],
            fetched_at_ms: Some(0),
            operator_grid: None,
        })
    }
}

/// Offline propagation/space-weather read mock for the tier-2 testserver
/// (phase 3.2). `predict_path` returns a deterministic prediction with
/// `tx_grid="CN87"` (4-char provenance) + one channel with 24-long hourly
/// vectors; `solar` returns a fixed snapshot. Read-only — never touches the
/// guard.
pub struct MockPrediction;

#[async_trait]
impl PredictionPort for MockPrediction {
    async fn predict_path(&self, _req: PredictRequestDto) -> Result<PathPredictionDto, PortError> {
        Ok(PathPredictionDto {
            bearing_deg: 90.0,
            distance_km: 4000.0,
            ssn: 70.0,
            year: 2026,
            month: 6,
            tx_grid: SEED_TX_GRID.into(),
            channels: vec![ChannelReliabilityDto {
                frequency_khz: SEED_GW_FREQ_KHZ,
                rel_by_hour: vec![0.5; 24],
                snr_by_hour: vec![10.0; 24],
                mufday_by_hour: vec![0.8; 24],
            }],
        })
    }
    async fn solar(&self) -> Result<SolarSnapshotDto, PortError> {
        Ok(SolarSnapshotDto {
            sfi: Some(140.0),
            a_index: Some(7.0),
            k_index: Some(2.0),
            ssn: 70.0,
            updated_at_ms: 0,
            source: "bundled".into(),
        })
    }
}

/// A mock [`ProvisionPort`]. The probes report the engine bundled + not-yet-ready
/// with one pending checkpoint; `vara_install_start` returns a green summary.
/// Non-transmit, ungated — never touches the guard.
pub struct MockProvision;

#[async_trait]
impl ProvisionPort for MockProvision {
    async fn vara_engine_available(&self) -> Result<bool, PortError> {
        Ok(true)
    }
    async fn vara_install_status(&self) -> Result<VaraInstallStatusDto, PortError> {
        Ok(VaraInstallStatusDto {
            ready: false,
            checkpoints: vec![VaraCheckpointDto {
                id: Some("deps".into()),
                index: Some(1),
                total: Some(7),
                state: Some("pending".into()),
                detail: None,
            }],
        })
    }
    async fn vara_install_start(
        &self,
        _installer_path: String,
    ) -> Result<VaraInstallSummaryDto, PortError> {
        Ok(VaraInstallSummaryDto {
            ok: true,
            prefix: Some("/home/ham/.wine-vara".into()),
            vara_version: Some("VARA HF".into()),
        })
    }
}
