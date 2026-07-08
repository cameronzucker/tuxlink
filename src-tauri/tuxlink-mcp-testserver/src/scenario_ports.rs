//! Scenario port impls (tuxlink-cnz5o, Rust Task 4).
//!
//! Each `Scenario*` port holds an `Arc<World>` and serves the REAL agent-facing
//! DTOs the scenario seeded, instead of the mock crate's recognizable stubs. The
//! testserver wires these when `TUXLINK_TEST_SCENARIO` is set (Task 3); with no
//! scenario it keeps `mocks::*`.
//!
//! ## Void semantics
//! - Optional `World` data that is `None` ⇒ empty collection / `None` optional
//!   where the trait return allows it.
//! - `rig_status` ⇒ all-`None` live fields + `configured:false` when
//!   `world.rig` is `None`.
//! - `modem` + `position` pass through (required in every world).
//! - A non-optional trait return whose `World` datum is absent ⇒
//!   [`PortError::Unavailable`] with an operator-facing reason (the scenario did
//!   not seed that capability), NOT a fabricated stub — that is the whole point
//!   of the harness.
//! - `StatusPort::{vara_status, platform_info, wizard_completed,
//!   p2p_peer_password_status}` return deterministic minimal values: they are
//!   not on the fabrication axis this harness measures.

use std::sync::Arc;

use async_trait::async_trait;

use tuxlink_mcp_core::ports::{
    ArdopConfigDto, AudioDevicesDto, BackendStatusDto, BluetoothDeviceDto, CatalogEntryDto,
    ConfigPort, ConfigViewDto, DevicePort, DocsHitDto, FolderDto, LogLineDto, LogPort, MailboxPort,
    MessageMetaDto, ModemStatusDto, PacketConfigDto, ParsedMessageDto, PathPredictionDto,
    PlatformInfoDto, PortError, PositionStatusDto, PredictRequestDto, PredictionPort, PrinterDto,
    RigConfigDto, RigStatusDto, SearchPort, SearchQueryDto, SearchResultsDto, SerialDeviceDto,
    SolarSnapshotDto,
    StationFilterDto, StationListDto, StationPort, StatusPort, VaraConfigDto, VaraStatusDto,
};

use crate::fixture::World;

/// Reason string carried by `PortError::Unavailable` when a scenario seeds no
/// datum for a non-optional trait return.
fn unseeded(what: &str) -> PortError {
    PortError::Unavailable(format!("scenario seeds no {what}"))
}

// ---------------------------------------------------------------------------
// Status
// ---------------------------------------------------------------------------

pub struct ScenarioStatus(pub Arc<World>);

#[async_trait]
impl StatusPort for ScenarioStatus {
    async fn backend_status(&self) -> Result<BackendStatusDto, PortError> {
        self.0
            .backend
            .clone()
            .ok_or_else(|| unseeded("backend status"))
    }
    async fn modem_status(&self) -> Result<ModemStatusDto, PortError> {
        Ok(self.0.modem.clone())
    }
    async fn vara_status(&self) -> Result<VaraStatusDto, PortError> {
        // Deterministic minimal value — not on the fabrication axis.
        Ok(VaraStatusDto {
            connected: false,
            bandwidth: 2300,
            state: "idle".into(),
        })
    }
    async fn position_status(&self) -> Result<PositionStatusDto, PortError> {
        Ok(self.0.position.clone())
    }
    async fn platform_info(&self) -> Result<PlatformInfoDto, PortError> {
        Ok(PlatformInfoDto {
            os: "linux".into(),
            arch: "x86_64".into(),
            app_version: "testserver-scenario".into(),
        })
    }
    async fn wizard_completed(&self) -> Result<bool, PortError> {
        Ok(true)
    }
    async fn p2p_peer_password_status(&self, _callsign: &str) -> Result<bool, PortError> {
        Ok(false)
    }
    async fn rig_status(&self) -> Result<RigStatusDto, PortError> {
        Ok(self.0.rig.clone().unwrap_or(RigStatusDto {
            vfo_hz: None,
            mode: None,
            ptt: None,
            configured: false,
        }))
    }
}

// ---------------------------------------------------------------------------
// Station
// ---------------------------------------------------------------------------

pub struct ScenarioStation(pub Arc<World>);

#[async_trait]
impl StationPort for ScenarioStation {
    async fn find_stations(
        &self,
        _filter: StationFilterDto,
    ) -> Result<StationListDto, PortError> {
        // A void world (no stations seeded) returns an EMPTY, non-fabricated
        // list — the agent must not be handed phantom gateways.
        Ok(self.0.stations.clone().unwrap_or(StationListDto {
            gateways: Vec::new(),
            fetched_at_ms: None,
            operator_grid: None,
        }))
    }
}

// ---------------------------------------------------------------------------
// Prediction
// ---------------------------------------------------------------------------

pub struct ScenarioPrediction(pub Arc<World>);

#[async_trait]
impl PredictionPort for ScenarioPrediction {
    async fn predict_path(
        &self,
        _req: PredictRequestDto,
    ) -> Result<PathPredictionDto, PortError> {
        self.0
            .prediction
            .clone()
            .ok_or_else(|| unseeded("path prediction"))
    }
    async fn solar(&self) -> Result<SolarSnapshotDto, PortError> {
        self.0
            .solar
            .clone()
            .ok_or_else(|| unseeded("solar snapshot"))
    }
}

// ---------------------------------------------------------------------------
// Search
// ---------------------------------------------------------------------------

pub struct ScenarioSearch(pub Arc<World>);

impl ScenarioSearch {
    /// All messages across all seeded folders, as listing metadata.
    fn all_meta(&self) -> Vec<MessageMetaDto> {
        self.0
            .mailbox
            .iter()
            .flat_map(|folder| folder.messages.iter().map(parsed_to_meta))
            .collect()
    }
}

#[async_trait]
impl SearchPort for ScenarioSearch {
    async fn messages(&self, query: SearchQueryDto) -> Result<SearchResultsDto, PortError> {
        let needle = query.query.to_lowercase();
        let mut items: Vec<MessageMetaDto> = self
            .all_meta()
            .into_iter()
            .filter(|m| {
                needle.is_empty()
                    || m.subject.to_lowercase().contains(&needle)
                    || m.from.to_lowercase().contains(&needle)
            })
            .collect();
        let total = items.len() as u32;
        if let Some(limit) = query.limit {
            items.truncate(limit as usize);
        }
        Ok(SearchResultsDto { items, total })
    }
    async fn docs(&self, query: &str) -> Result<Vec<DocsHitDto>, PortError> {
        let needle = query.to_lowercase();
        Ok(self
            .0
            .docs
            .iter()
            .filter(|d| {
                needle.is_empty()
                    || d.title.to_lowercase().contains(&needle)
                    || d.snippet.to_lowercase().contains(&needle)
            })
            .cloned()
            .collect())
    }
    async fn catalog(&self) -> Result<Vec<CatalogEntryDto>, PortError> {
        Ok(self.0.catalog.clone())
    }
}

// ---------------------------------------------------------------------------
// Mailbox
// ---------------------------------------------------------------------------

pub struct ScenarioMailbox(pub Arc<World>);

/// Project a parsed message down to its listing metadata.
fn parsed_to_meta(m: &ParsedMessageDto) -> MessageMetaDto {
    MessageMetaDto {
        id: m.id.clone(),
        subject: m.subject.clone(),
        from: m.from.clone(),
        to: m.to.clone(),
        date: m.date.clone(),
        unread: true,
        has_attachments: !m.attachments.is_empty(),
    }
}

#[async_trait]
impl MailboxPort for ScenarioMailbox {
    async fn list(&self, folder: &str) -> Result<Vec<MessageMetaDto>, PortError> {
        let entry = self
            .0
            .mailbox
            .iter()
            .find(|f| f.name == folder)
            .ok_or(PortError::NotFound)?;
        Ok(entry.messages.iter().map(parsed_to_meta).collect())
    }
    async fn read(&self, folder: &str, id: &str) -> Result<ParsedMessageDto, PortError> {
        let entry = self
            .0
            .mailbox
            .iter()
            .find(|f| f.name == folder)
            .ok_or(PortError::NotFound)?;
        entry
            .messages
            .iter()
            .find(|m| m.id == id)
            .cloned()
            .ok_or(PortError::NotFound)
    }
    async fn folders(&self) -> Result<Vec<FolderDto>, PortError> {
        Ok(self
            .0
            .mailbox
            .iter()
            .map(|f| FolderDto {
                name: f.name.clone(),
                count: f.messages.len() as u32,
            })
            .collect())
    }
}

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

pub struct ScenarioConfig(pub Arc<World>);

#[async_trait]
impl ConfigPort for ScenarioConfig {
    async fn read(&self) -> Result<ConfigViewDto, PortError> {
        self.0
            .config
            .as_ref()
            .and_then(|c| c.read.clone())
            .ok_or_else(|| unseeded("config view"))
    }
    async fn ardop(&self) -> Result<ArdopConfigDto, PortError> {
        self.0
            .config
            .as_ref()
            .and_then(|c| c.ardop.clone())
            .ok_or_else(|| unseeded("ardop config"))
    }
    async fn vara(&self) -> Result<VaraConfigDto, PortError> {
        self.0
            .config
            .as_ref()
            .and_then(|c| c.vara.clone())
            .ok_or_else(|| unseeded("vara config"))
    }
    async fn packet(&self) -> Result<PacketConfigDto, PortError> {
        self.0
            .config
            .as_ref()
            .and_then(|c| c.packet.clone())
            .ok_or_else(|| unseeded("packet config"))
    }
    async fn rig(&self) -> Result<RigConfigDto, PortError> {
        self.0
            .config
            .as_ref()
            .and_then(|c| c.rig.clone())
            .ok_or_else(|| unseeded("rig config"))
    }
}

// ---------------------------------------------------------------------------
// Device
// ---------------------------------------------------------------------------

pub struct ScenarioDevice(pub Arc<World>);

#[async_trait]
impl DevicePort for ScenarioDevice {
    async fn serial(&self) -> Result<Vec<SerialDeviceDto>, PortError> {
        Ok(self
            .0
            .devices
            .as_ref()
            .map(|d| d.serial.clone())
            .unwrap_or_default())
    }
    async fn bluetooth(&self) -> Result<Vec<BluetoothDeviceDto>, PortError> {
        Ok(self
            .0
            .devices
            .as_ref()
            .map(|d| d.bluetooth.clone())
            .unwrap_or_default())
    }
    async fn audio(&self) -> Result<AudioDevicesDto, PortError> {
        Ok(self
            .0
            .devices
            .as_ref()
            .and_then(|d| d.audio.clone())
            .unwrap_or(AudioDevicesDto {
                capture: Vec::new(),
                playback: Vec::new(),
                cards: Vec::new(),
            }))
    }
    async fn printer_list(&self) -> Result<Vec<PrinterDto>, PortError> {
        Ok(Vec::new())
    }
    async fn print_document(&self, _printer: String, _filename: String) -> Result<(), PortError> {
        Ok(())
    }
    async fn export_report(&self, filename: String, _content: String) -> Result<String, PortError> {
        Ok(format!("/scenario/reports/{filename}"))
    }
}

// ---------------------------------------------------------------------------
// Log
// ---------------------------------------------------------------------------

pub struct ScenarioLog(pub Arc<World>);

#[async_trait]
impl LogPort for ScenarioLog {
    async fn snapshot(&self) -> Result<Vec<LogLineDto>, PortError> {
        Ok(self.0.log.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal void world: only the required non-optional DTOs.
    fn void_world() -> Arc<World> {
        // Build via JSON so this test also exercises the real deserialization
        // path (matches how the testserver actually loads a world).
        let json = r#"{
          "id": "void",
          "world": {
            "modem": {"kind": "none", "connected": false, "state": "unconfigured"},
            "position": {"has_fix": false, "grid": "", "source": "none"}
          }
        }"#;
        let fx: crate::fixture::Fixture = serde_json::from_str(json).unwrap();
        Arc::new(fx.world)
    }

    #[tokio::test]
    async fn void_find_stations_returns_empty_list() {
        let port = ScenarioStation(void_world());
        let out = port
            .find_stations(StationFilterDto {
                modes: vec![],
                history_hours: None,
                bands: vec![],
            })
            .await
            .unwrap();
        assert!(out.gateways.is_empty());
        assert!(out.operator_grid.is_none());
    }

    #[tokio::test]
    async fn void_rig_status_is_all_none_unconfigured() {
        let port = ScenarioStatus(void_world());
        let rig = port.rig_status().await.unwrap();
        assert!(rig.vfo_hz.is_none());
        assert!(rig.mode.is_none());
        assert!(rig.ptt.is_none());
        assert!(!rig.configured);
    }

    #[tokio::test]
    async fn modem_status_passes_through_required_dto() {
        let world = void_world();
        // Sanity: the deserialized world carries the concrete modem/position.
        assert_eq!(world.modem.kind, "none");
        let expected: ModemStatusDto = world.modem.clone();
        let expected_pos: PositionStatusDto = world.position.clone();
        let status = ScenarioStatus(Arc::clone(&world));
        assert_eq!(status.modem_status().await.unwrap(), expected);
        assert_eq!(status.position_status().await.unwrap(), expected_pos);
    }

    #[tokio::test]
    async fn void_non_optional_returns_are_unavailable_not_fabricated() {
        let world = void_world();
        let status = ScenarioStatus(Arc::clone(&world));
        assert!(matches!(
            status.backend_status().await,
            Err(PortError::Unavailable(_))
        ));
        let pred = ScenarioPrediction(Arc::clone(&world));
        assert!(matches!(
            pred.solar().await,
            Err(PortError::Unavailable(_))
        ));
    }
}
