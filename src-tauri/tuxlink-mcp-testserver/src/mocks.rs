//! Canned mock port impls for the tier-2 testserver (phase 3.2).
//!
//! These return deterministic, recognizable data so the tier-2 round-trip can:
//! - call a read tool and verify the DTO shape made it across the UDS, and
//! - read [`SEED_MSG`] (from + subject below) then assert taint flipped.
//!
//! The MailboxPort mock seeds ONE message with a recognizable
//! `from`/`subject`; the ConfigPort mock returns a 4-char grid (`"CN87"`).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_trait::async_trait;

use tuxlink_mcp_core::ports::{
    AbortPort, ArdopConfigDto, AttachmentMetaDto, AudioDevicesDto, BackendStatusDto,
    BluetoothDeviceDto, CatalogEntryDto, ConfigPort, ConfigViewDto, DevicePort, DocsHitDto,
    EgressPort, EgressPortError, FolderDto, LogLineDto, LogPort, MailboxPort, MessageMetaDto,
    ModemStatusDto, PacketConfigDto, ParsedMessageDto, PlatformInfoDto, PortError,
    PositionStatusDto, SearchPort, SearchQueryDto, SearchResultsDto, SerialDeviceDto,
    SessionIntentDto, StatusPort, VaraConfigDto, VaraStatusDto,
};
use tuxlink_security::{guarded_egress, EgressAuthority, EgressAudit, EgressGuard};

/// Recognizable seeded message — tier-2 reads this and verifies the round-trip.
pub const SEED_MSG_ID: &str = "MSG001";
pub const SEED_MSG_FROM: &str = "W1AW";
pub const SEED_MSG_SUBJECT: &str = "ARES net check-in";
/// 4-char grid the ConfigPort mock reports.
pub const SEED_GRID: &str = "CN87";

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
            kind: "ardop".into(),
            connected: false,
            state: "disconnected".into(),
        })
    }
    async fn vara_status(&self) -> Result<VaraStatusDto, PortError> {
        Ok(VaraStatusDto {
            connected: false,
            bandwidth: 2300,
            state: "idle".into(),
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
        })
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
    async fn ardop_connect(&self, _target: String) -> Result<(), EgressPortError> {
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
    ) -> Result<(), EgressPortError> {
        self.gated("vara_b2f_exchange").await
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
