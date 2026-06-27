//! MCP server core for the agent caller (phase 3.1 transport spine).
//!
//! This crate exposes the Tuxlink MCP endpoint as a standalone, Pi-buildable
//! library so BOTH the Tauri monolith AND the tier-2 testserver can serve the
//! SAME real router over the SAME real [`tuxlink_security::EgressGuard`] without
//! pulling in the Tauri app.
//!
//! Phase 3.1 ships exactly ONE inert tool — [`router::TuxlinkMcp::server_info`]
//! — which reports the app name/version plus the live `EgressGuard` arm/taint
//! state, proving the spine reaches real security state. No real capability is
//! wired here; later phases add tools, redaction, taint-setting, and the egress
//! gate.
//!
//! Design seam: all of `server_info`'s logic lives in the pure, transport-free
//! [`server_info_view`] free function so it is unit-testable WITHOUT the rmcp
//! transport. The `#[tool]` method in [`router`] is a thin wrapper over it,
//! mirroring the project's core-fn + thin-adapter pattern.

use std::sync::Arc;

use serde::Serialize;

use tuxlink_security::EgressGuard;

pub mod content;
pub mod ports;
pub mod router;
pub mod transport_uds;
pub mod validate;

pub use ports::{
    AbortPort, ComposePort, ConfigPort, DevicePort, EgressPort, EgressPortError, LogPort,
    MailboxPort, PortError, SearchPort, StatusPort, WritePort, WritePortError,
};
pub use router::TuxlinkMcp;
pub use transport_uds::serve;

/// The live handles the MCP router needs. Phase 3.1's only tool (`server_info`)
/// reads the [`EgressGuard`] plus the embedder-injected app identity; later
/// phases (3.2+) extend this bundle with the backend, session-log, modem, and
/// position handles as tools are added.
///
/// Embedders inject identity: the monolith passes `env!("CARGO_PKG_NAME")` /
/// `env!("CARGO_PKG_VERSION")` so `server_info` reports the real Tuxlink app
/// version, NOT this core crate's own package identity.
#[derive(Clone)]
pub struct McpState {
    /// The armed-grant + taint authority, shared with the Tauri-managed
    /// `Arc<EgressGuard>` (lib.rs `.manage()`).
    pub guard: Arc<EgressGuard>,
    /// The embedding app's package name (e.g. `"tuxlink"`), injected by the
    /// embedder. `server_info` echoes this — it must NOT be the core crate's
    /// `CARGO_PKG_NAME`.
    pub name: String,
    /// The embedding app's package version, injected by the embedder.
    /// `server_info` echoes this — it must NOT be the core crate's
    /// `CARGO_PKG_VERSION`.
    pub version: String,
    /// Status + diagnostic reads (backend/modem/vara/position/platform/wizard,
    /// p2p peer-password status). None taint.
    pub status: Arc<dyn StatusPort>,
    /// Mailbox reads. `list`/`read` taint at the calling tool.
    pub mailbox: Arc<dyn MailboxPort>,
    /// Search across mailbox/docs/catalog. `messages` taints at the calling
    /// tool.
    pub search: Arc<dyn SearchPort>,
    /// Curated, non-secret config reads.
    pub config: Arc<dyn ConfigPort>,
    /// Hardware device enumeration.
    pub devices: Arc<dyn DevicePort>,
    /// Session-log snapshot. Taints at the calling tool.
    pub logs: Arc<dyn LogPort>,
    /// GATED egress capability (CMS/P2P/ARDOP/VARA/packet connect + B2F). Each
    /// impl method gates itself through `guarded_egress(.., Agent, ..)`, so a
    /// disarmed/expired/tainted/poisoned session cannot transmit.
    pub egress: Arc<dyn EgressPort>,
    /// UNGATED pure-stop capability. Stopping is always allowed.
    pub abort: Arc<dyn AbortPort>,
    /// GATED config/state writes (modem/grid/privacy/mailbox/attachment). Each
    /// impl validates input first, then gates the mutation through
    /// `guarded_egress(.., Agent, ..)`.
    pub write: Arc<dyn WritePort>,
    /// UNGATED compose/staging capability. Stages local outbox drafts; no
    /// transmission until a later gated connect. Validates input; never gates,
    /// never taints.
    pub compose: Arc<dyn ComposePort>,
}

/// Serializable shape returned by the `server_info` tool.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ServerInfoDto {
    /// Embedding app's package name (injected via [`McpState::name`]).
    pub name: String,
    /// Embedding app's package version (injected via [`McpState::version`]).
    pub version: String,
    /// True when send authority is currently armed (grant not expired).
    pub armed: bool,
    /// True when the session is tainted by untrusted content.
    pub tainted: bool,
}

/// Pure view of `server_info`: reads the live guard state and the
/// embedder-injected app identity. Transport-free so it can be unit-tested
/// directly. `name`/`version` echo the identity the embedder set on
/// [`McpState`] (the app's, not this core crate's); `armed` is
/// `armed_remaining() > 0` (a live, un-expired grant); `tainted` mirrors the
/// guard's taint flag.
pub fn server_info_view(state: &McpState) -> ServerInfoDto {
    ServerInfoDto {
        name: state.name.clone(),
        version: state.version.clone(),
        armed: state.guard.armed_remaining() > 0,
        tainted: state.guard.is_tainted(),
    }
}

/// In-crate mock ports + test-state builders, shared by the `lib.rs`
/// `server_info_view` tests and the `router.rs` taint/round-trip tier-1 tests.
/// Gated on `#[cfg(test)]` so it never ships in a release build.
#[cfg(test)]
pub(crate) mod test_support {
    use std::sync::Arc;

    use async_trait::async_trait;

    use std::sync::atomic::{AtomicBool, Ordering};

    use crate::ports::{
        AbortPort, ArdopConfigDto, ArdopWriteDto, AttachmentMetaDto, AudioDevicesDto,
        BackendStatusDto, BluetoothDeviceDto, CatalogEntryDto, ComposeDraftDto, ComposePort,
        ConfigPort, ConfigViewDto, DevicePort, DocsHitDto, EgressPort, EgressPortError, FolderDto,
        GribRequestDto, LogLineDto, LogPort, MailboxPort, MessageMetaDto, ModemStatusDto,
        PacketConfigDto, PacketWriteDto, ParsedMessageDto, PlatformInfoDto, PortError,
        PositionStatusDto, SearchPort, SearchQueryDto, SearchResultsDto, SendFormDto,
        SerialDeviceDto, SessionIntentDto, StatusPort, VaraConfigDto, VaraStatusDto, VaraWriteDto,
        WritePort, WritePortError,
    };
    use crate::validate::{
        validate_address, validate_attachment_dest, validate_body, validate_drive_level,
        validate_subject, validate_vara_bandwidth,
    };
    use crate::McpState;
    use tuxlink_security::{guarded_egress, EgressAuthority, EgressGuard};

    /// A recognizable seeded message so taint-then-read tier-2 checks can assert
    /// the sender + subject round-tripped. Mirrors the testserver fixture.
    pub const SEED_MSG_ID: &str = "MSG001";
    pub const SEED_MSG_FROM: &str = "W1AW";
    pub const SEED_MSG_SUBJECT: &str = "ARES net check-in";

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
                grid: "CN87".into(),
                source: "gps".into(),
            })
        }
        async fn platform_info(&self) -> Result<PlatformInfoDto, PortError> {
            Ok(PlatformInfoDto {
                os: "linux".into(),
                arch: "aarch64".into(),
                app_version: "9.9.9".into(),
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
                grid: "CN87".into(),
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

    /// A mock [`EgressPort`] that gates EVERY method through the REAL
    /// [`guarded_egress`] against a SHARED [`EgressGuard`], flipping a shared
    /// `op_ran` flag inside the gated op. So a test can assert both the
    /// authorization decision AND whether the underlying egress actually fired
    /// (denied → flag stays false; allowed → flag flips true). `Denied` is
    /// surfaced as [`EgressPortError::Denied`] with the gate's reason.
    pub struct MockEgress {
        guard: Arc<EgressGuard>,
        op_ran: Arc<AtomicBool>,
    }

    impl MockEgress {
        pub fn new(guard: Arc<EgressGuard>, op_ran: Arc<AtomicBool>) -> Self {
            Self { guard, op_ran }
        }

        /// Run `label` through the real gate; flip `op_ran` only if it runs.
        async fn gated(&self, label: &str) -> Result<(), EgressPortError> {
            let op_ran = Arc::clone(&self.op_ran);
            let noop_audit = |_a: tuxlink_security::EgressAudit<'_>| {};
            guarded_egress(
                &self.guard,
                EgressAuthority::Agent,
                label,
                &noop_audit,
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

    /// A mock [`AbortPort`] that flips a shared `aborted` flag and is NEVER
    /// gated — every method succeeds regardless of guard state.
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

    /// A mock [`WritePort`] proving the **validate-before-gate** contract: every
    /// method runs the relevant `validate.rs` check FIRST (a bad value returns
    /// [`WritePortError::Invalid`] WITHOUT touching the gate, so the armed grant
    /// is not consumed and `op_ran` never flips), then gates the mutation through
    /// the REAL [`guarded_egress`] against the SHARED [`EgressGuard`], flipping
    /// `op_ran` only inside the gated op. `attachment_save` validates `dest`
    /// against a per-mock tempdir base.
    pub struct MockWrite {
        guard: Arc<EgressGuard>,
        op_ran: Arc<AtomicBool>,
        /// Attachment base dir for `attachment_save` dest validation. Kept alive
        /// (RAII) for the mock's lifetime.
        base: tempfile::TempDir,
    }

    impl MockWrite {
        pub fn new(guard: Arc<EgressGuard>, op_ran: Arc<AtomicBool>) -> Self {
            Self {
                guard,
                op_ran,
                base: tempfile::tempdir().expect("tempdir for attachment base"),
            }
        }

        /// Gate `label` through the real gate; flip `op_ran` only if it runs.
        async fn gated(&self, label: &str) -> Result<(), WritePortError> {
            let op_ran = Arc::clone(&self.op_ran);
            let noop_audit = |_a: tuxlink_security::EgressAudit<'_>| {};
            guarded_egress(
                &self.guard,
                EgressAuthority::Agent,
                label,
                &noop_audit,
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
            // VALIDATE BEFORE GATE: an out-of-range drive level is Invalid even
            // when disarmed; the `?` returns before `gated` is reached.
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
            // VALIDATE BEFORE GATE: a traversal/absolute/escaping dest is Invalid
            // even when disarmed.
            let path = validate_attachment_dest(self.base.path(), &dest)?;
            self.gated("attachment_save").await?;
            Ok(path.to_string_lossy().into_owned())
        }
    }

    /// A mock [`ComposePort`] proving the UNGATED-compose contract: it validates
    /// recipients/subject/body and, on success, flips a shared `staged` flag and
    /// returns a canned MID. It NEVER touches the guard (no `op_ran`, no taint),
    /// so a compose succeeds without an arm and cannot be `Denied`.
    pub struct MockCompose {
        staged: Arc<AtomicBool>,
    }

    impl MockCompose {
        pub fn new(staged: Arc<AtomicBool>) -> Self {
            Self { staged }
        }
        fn validate_recipients(
            to: &[String],
            cc: &[String],
        ) -> Result<(), WritePortError> {
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
        async fn catalog_send_inquiry(
            &self,
            _item_ids: Vec<String>,
        ) -> Result<String, WritePortError> {
            Ok(self.stage())
        }
        async fn grib_send_request(&self, dto: GribRequestDto) -> Result<String, WritePortError> {
            validate_subject(&dto.subject)?;
            Ok(self.stage())
        }
    }

    /// Build an [`McpState`] around the supplied guard, wiring all mock ports.
    /// The egress/abort flags are internal; use [`state_with_egress_probes`] to
    /// observe whether a gated egress op actually ran or an abort fired.
    pub fn state_with_guard(guard: EgressGuard) -> McpState {
        state_with_egress_probes(guard).0
    }

    /// Like [`state_with_guard`] but also returns `(op_ran, aborted)` probe
    /// flags so the egress-gate + injection-containment tests can assert the
    /// underlying op fired (or did not) and that abort succeeded.
    ///
    /// `op_ran` is SHARED by the egress mock AND the write mock (both flip it
    /// inside their gated op), so a write-tier test can assert the gated
    /// mutation ran or did not.
    pub fn state_with_egress_probes(
        guard: EgressGuard,
    ) -> (McpState, Arc<AtomicBool>, Arc<AtomicBool>) {
        let (state, op_ran, aborted, _staged) = state_with_all_probes(guard);
        (state, op_ran, aborted)
    }

    /// Full probe builder: returns `(state, op_ran, aborted, staged)`. `op_ran`
    /// is shared by the egress + write mocks (flipped inside the gated op);
    /// `staged` is flipped by the ungated compose mock on a successful stage.
    pub fn state_with_all_probes(
        guard: EgressGuard,
    ) -> (McpState, Arc<AtomicBool>, Arc<AtomicBool>, Arc<AtomicBool>) {
        let guard = Arc::new(guard);
        let op_ran = Arc::new(AtomicBool::new(false));
        let aborted = Arc::new(AtomicBool::new(false));
        let staged = Arc::new(AtomicBool::new(false));
        let state = McpState {
            guard: Arc::clone(&guard),
            name: "tuxlink".into(),
            version: "9.9.9".into(),
            status: Arc::new(MockStatus),
            mailbox: Arc::new(MockMailbox),
            search: Arc::new(MockSearch),
            config: Arc::new(MockConfig),
            devices: Arc::new(MockDevice),
            logs: Arc::new(MockLog),
            egress: Arc::new(MockEgress::new(Arc::clone(&guard), Arc::clone(&op_ran))),
            abort: Arc::new(MockAbort::new(Arc::clone(&aborted))),
            write: Arc::new(MockWrite::new(Arc::clone(&guard), Arc::clone(&op_ran))),
            compose: Arc::new(MockCompose::new(Arc::clone(&staged))),
        };
        (state, op_ran, aborted, staged)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Deterministic clock so an armed grant has a known, un-expired deadline.
    fn fixed_1000() -> u64 {
        1000
    }

    fn state_with(guard: EgressGuard) -> McpState {
        // Inject identity distinct from this core crate's own 0.0.0 so a
        // regression to env!("CARGO_PKG_NAME"/"CARGO_PKG_VERSION") would be
        // caught by `view_reports_package_identity`.
        crate::test_support::state_with_guard(guard)
    }

    #[test]
    fn view_reports_package_identity() {
        let state = state_with(EgressGuard::with_clock(fixed_1000));
        let dto = server_info_view(&state);
        // The DTO must echo the embedder-injected identity, NOT the core
        // crate's CARGO_PKG_* (which are tuxlink-mcp-core / 0.0.0).
        assert_eq!(dto.name, "tuxlink");
        assert_eq!(dto.version, "9.9.9");
    }

    #[test]
    fn fresh_guard_is_not_armed_and_not_tainted() {
        let state = state_with(EgressGuard::with_clock(fixed_1000));
        let dto = server_info_view(&state);
        assert!(!dto.armed, "a fresh guard must report armed=false");
        assert!(!dto.tainted, "a fresh guard must report tainted=false");
    }

    #[test]
    fn arming_makes_view_report_armed() {
        let state = state_with(EgressGuard::with_clock(fixed_1000));
        state.guard.arm(30); // deadline 1030, now 1000 -> 30s remaining
        let dto = server_info_view(&state);
        assert!(dto.armed, "after arm(30) the view must report armed=true");
        assert!(!dto.tainted);
    }

    #[test]
    fn expired_grant_is_not_armed() {
        // arm(0): deadline == now == 1000 -> armed_remaining() == 0 -> not armed.
        let state = state_with(EgressGuard::with_clock(fixed_1000));
        state.guard.arm(0);
        let dto = server_info_view(&state);
        assert!(!dto.armed, "an expired/zero grant must report armed=false");
    }

    #[test]
    fn tainting_makes_view_report_tainted() {
        let state = state_with(EgressGuard::with_clock(fixed_1000));
        state.guard.taint();
        let dto = server_info_view(&state);
        assert!(dto.tainted, "after taint() the view must report tainted=true");
    }

    #[test]
    fn armed_and_tainted_are_independent() {
        // Taint must not clear the arm grant, and vice versa: both can be true.
        let state = state_with(EgressGuard::with_clock(fixed_1000));
        state.guard.arm(30);
        state.guard.taint();
        let dto = server_info_view(&state);
        assert!(dto.armed);
        assert!(dto.tainted);
    }
}
