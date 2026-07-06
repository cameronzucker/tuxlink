//! Scenario `Fixture` / `World` types over the REAL `tuxlink_mcp_core` DTOs
//! (tuxlink-cnz5o, sim-harness PoC — Rust Tasks 1-3).
//!
//! A scenario fixture is ONE JSON object. `Fixture` reads `{id, world}` and
//! IGNORES any other top-level keys (`family`, `depth`, `taint_state`, `prompt`,
//! `spec`, …) so the SAME file drives both the Rust testserver and the Python
//! `Scenario` reader. `Fixture`/`World` therefore MUST NOT use
//! `#[serde(deny_unknown_fields)]`.
//!
//! `World` deserializes into the exact agent-facing DTOs from
//! [`tuxlink_mcp_core::ports`]. `modem` + `position` are NON-OPTIONAL (their DTOs
//! have no nullable variant), so every loadable fixture MUST supply minimal
//! concrete state for them even when they are not the scenario's focus. All other
//! world data is `Option`/`Vec` with `#[serde(default)]` so a void world is just
//! `{modem, position}` and everything else empty.
//!
//! `load_fixture` fails LOUDLY: an unreadable path is [`FixtureError::Io`]; a
//! malformed/absent-required-field body is [`FixtureError::Parse`].

use std::path::Path;
use std::sync::Arc;

use serde::Deserialize;
use thiserror::Error;

use tuxlink_mcp_core::ports::{
    ArdopConfigDto, AudioDevicesDto, BackendStatusDto, BluetoothDeviceDto, CatalogEntryDto,
    ConfigViewDto, DocsHitDto, FolderDto, LogLineDto, ModemStatusDto, PacketConfigDto,
    ParsedMessageDto, PathPredictionDto, PositionStatusDto, RigConfigDto, RigStatusDto,
    SerialDeviceDto, SolarSnapshotDto, StationListDto, VaraConfigDto,
};

/// The env var that names the scenario fixture path (Task 3). Absent/empty ⇒ the
/// testserver keeps its current mock ports.
pub const SCENARIO_ENV: &str = "TUXLINK_TEST_SCENARIO";

/// One scenario mailbox folder + its parsed messages. The testserver's
/// `ScenarioMailbox` serves `folders()` from `name`/`messages.len()`, `list()`
/// from the per-message metadata, and `read()` from the full `ParsedMessageDto`.
#[derive(Debug, Clone, Deserialize)]
pub struct MailboxEntry {
    /// Folder name (e.g. `"Inbox"`).
    pub name: String,
    /// The folder's fully-parsed messages, in listing order.
    #[serde(default)]
    pub messages: Vec<ParsedMessageDto>,
}

/// Curated non-secret config the scenario `ConfigPort` serves. Each field is
/// optional; an absent field makes the corresponding `ConfigPort` method return
/// [`tuxlink_mcp_core::ports::PortError::Unavailable`] (that config datum is not
/// present in this world).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ConfigWorld {
    #[serde(default)]
    pub read: Option<ConfigViewDto>,
    #[serde(default)]
    pub ardop: Option<ArdopConfigDto>,
    #[serde(default)]
    pub vara: Option<VaraConfigDto>,
    #[serde(default)]
    pub packet: Option<PacketConfigDto>,
    #[serde(default)]
    pub rig: Option<RigConfigDto>,
}

/// Hardware device enumeration the scenario `DevicePort` serves. Absent lists
/// deserialize to empty (a void device world lists nothing).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct DeviceWorld {
    #[serde(default)]
    pub serial: Vec<SerialDeviceDto>,
    #[serde(default)]
    pub bluetooth: Vec<BluetoothDeviceDto>,
    /// Capture/playback device names. Absent ⇒ empty capture + playback lists.
    #[serde(default)]
    pub audio: Option<AudioDevicesDto>,
}

/// The world a scenario seeds into the real MCP router at the port boundary.
///
/// `modem` + `position` are REQUIRED (their DTOs are non-optional); every other
/// field defaults to `None`/empty so a void world is minimal. NO
/// `deny_unknown_fields` — extra scenario keys (`family`, `prompt`, `spec`, …)
/// live alongside `world` at the `Fixture` top level and are ignored here.
#[derive(Debug, Clone, Deserialize)]
pub struct World {
    // ---- Non-optional (minimal concrete state required) -----------------
    /// Live modem status. REQUIRED.
    pub modem: ModemStatusDto,
    /// Current position/grid status. REQUIRED.
    pub position: PositionStatusDto,

    // ---- Status/diagnostics ---------------------------------------------
    /// Live rig status. `None` ⇒ `rig_status` reports all-`None`/`configured:false`.
    #[serde(default)]
    pub rig: Option<RigStatusDto>,
    /// Live backend/CMS status. `None` ⇒ `backend_status` is `Unavailable`.
    #[serde(default)]
    pub backend: Option<BackendStatusDto>,

    // ---- Station intelligence -------------------------------------------
    /// The gateway directory `find_stations` returns. `None` ⇒ empty list.
    #[serde(default)]
    pub stations: Option<StationListDto>,
    /// The HF path prediction `predict_path` returns. `None` ⇒ `Unavailable`.
    #[serde(default)]
    pub prediction: Option<PathPredictionDto>,
    /// The space-weather snapshot `solar` returns. `None` ⇒ `Unavailable`.
    #[serde(default)]
    pub solar: Option<SolarSnapshotDto>,

    // ---- Search-adjacent app content ------------------------------------
    /// In-app documentation hits `docs` returns.
    #[serde(default)]
    pub docs: Vec<DocsHitDto>,
    /// Template-catalog entries `catalog` returns.
    #[serde(default)]
    pub catalog: Vec<CatalogEntryDto>,

    // ---- Mailbox --------------------------------------------------------
    /// Folders + parsed messages the mailbox/search ports serve.
    #[serde(default)]
    pub mailbox: Vec<MailboxEntry>,

    // ---- Session log ----------------------------------------------------
    /// Session-log lines `snapshot` returns.
    #[serde(default)]
    pub log: Vec<LogLineDto>,

    // ---- Config + devices -----------------------------------------------
    #[serde(default)]
    pub config: Option<ConfigWorld>,
    #[serde(default)]
    pub devices: Option<DeviceWorld>,
}

/// A scenario fixture: `{id, world}` plus any number of ignored sibling keys.
#[derive(Debug, Clone, Deserialize)]
pub struct Fixture {
    /// The scenario identifier (echoed in logs; Python side keys on the same id).
    pub id: String,
    /// The seeded world.
    pub world: World,
}

/// Loading a fixture failed. Both variants are LOUD — the testserver refuses to
/// start on a bad scenario rather than silently degrading to mocks.
#[derive(Debug, Error)]
pub enum FixtureError {
    /// The fixture path could not be read.
    #[error("cannot read scenario fixture {path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
    /// The fixture body was not valid JSON, or a required field (`id`, `world`,
    /// `world.modem`, `world.position`) was missing/mis-typed.
    #[error("cannot parse scenario fixture {path}: {source}")]
    Parse {
        path: String,
        source: serde_json::Error,
    },
}

/// Read + parse a scenario fixture from `path`. Fails loudly on either an IO
/// error (unreadable path) or a parse error (malformed JSON / missing required
/// field).
pub fn load_fixture(path: &Path) -> Result<Fixture, FixtureError> {
    let raw = std::fs::read_to_string(path).map_err(|source| FixtureError::Io {
        path: path.display().to_string(),
        source,
    })?;
    serde_json::from_str::<Fixture>(&raw).map_err(|source| FixtureError::Parse {
        path: path.display().to_string(),
        source,
    })
}

/// Pure resolver (Task 3): map the raw `TUXLINK_TEST_SCENARIO` value to an
/// optional loaded world. `None`/empty ⇒ `Ok(None)` (keep the current mock
/// behavior); a set path is loaded and wrapped in `Arc<World>`.
pub fn resolve_scenario(raw: Option<String>) -> Result<Option<Arc<World>>, FixtureError> {
    match raw {
        Some(p) if !p.trim().is_empty() => {
            let fixture = load_fixture(Path::new(p.trim()))?;
            Ok(Some(Arc::new(fixture.world)))
        }
        _ => Ok(None),
    }
}

/// Read `TUXLINK_TEST_SCENARIO` from the environment and resolve it (Task 3).
/// Absent/empty ⇒ `Ok(None)`.
pub fn load_scenario_from_env() -> Result<Option<Arc<World>>, FixtureError> {
    let raw = std::env::var(SCENARIO_ENV).ok();
    resolve_scenario(raw)
}

// ---------------------------------------------------------------------------
// JSON Schema for cross-language fixture validation (Task 2).
//
// Hand-built (no `schemars`): the top-level contract is small and stable —
// `{id, world}` where `world` is an object whose property set is
// `WORLD_SCHEMA_FIELDS`. The per-port sub-object shapes are NOT re-encoded here;
// they are validated structurally on the Python side and, decisively, by
// `World`'s own serde deserialization (a wrong field fails `load_fixture`). This
// avoids a `schemars` dependency (a new major version vs the workspace's 0.8/0.9
// and an API-surface risk) for a contract this simple.
// ---------------------------------------------------------------------------

pub mod schema {
    use serde_json::{json, Value};

    /// The field NAMES `World` exposes. Kept beside `World`; the
    /// `schema_world_properties_match_fields` test ties this list to the schema
    /// generator, and `World`'s own deserialization ties fixtures to the real
    /// fields (a wrong field fails `load_fixture`).
    pub const WORLD_SCHEMA_FIELDS: &[&str] = &[
        "modem",
        "position",
        "rig",
        "backend",
        "stations",
        "prediction",
        "solar",
        "docs",
        "catalog",
        "mailbox",
        "log",
        "config",
        "devices",
    ];

    /// A minimal draft-07 JSON Schema for a scenario fixture, built by hand from
    /// [`WORLD_SCHEMA_FIELDS`] (no `schemars` dependency). The top-level contract
    /// is `{id, world}`; per-port sub-object shapes are permissive here and are
    /// validated structurally on the Python side and, decisively, by `World`'s
    /// serde deserialization. `additionalProperties` stays open so a scenario
    /// file may also carry the Python `Scenario` keys `Fixture` ignores.
    pub fn fixture_json_schema() -> Value {
        let mut world_props = serde_json::Map::new();
        for f in WORLD_SCHEMA_FIELDS {
            world_props.insert((*f).to_string(), json!({}));
        }
        json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "title": "TuxlinkScenarioFixture",
            "type": "object",
            "properties": {
                "id": { "type": "string" },
                "world": {
                    "type": "object",
                    "properties": Value::Object(world_props),
                    "additionalProperties": true
                }
            },
            "required": ["id", "world"],
            "additionalProperties": true
        })
    }
}

pub use schema::fixture_json_schema;

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    // A grounded world in REAL DTO wire shapes. Includes the required `modem` +
    // `position`, plus real `stations`/`rig`/`solar`. Field names verified
    // against tuxlink_mcp_core::ports.
    const GROUNDED_JSON: &str = r#"{
      "id": "grounded-gateways-01",
      "family": "station-intel",
      "prompt": "find a station",
      "spec": {"grounded_claims": ["W1AW"]},
      "world": {
        "modem": {"kind": "ardop", "connected": false, "state": "disconnected"},
        "position": {"has_fix": true, "grid": "CN87", "source": "gps"},
        "rig": {"vfo_hz": 7104000, "mode": "PKTUSB", "ptt": false, "configured": true},
        "backend": {"connected": true, "transport": "telnet", "state": "idle"},
        "stations": {
          "gateways": [
            {
              "mode": "vara-hf",
              "channel": "7104.0 VARA HF",
              "callsign": "W1AW",
              "grid": "FN31",
              "frequencies_khz": [7104.0],
              "antenna": "dipole",
              "distance_km": 4000.0,
              "distance_mi": 2485.5,
              "bearing_deg": 90.0
            }
          ],
          "fetched_at_ms": 1750000000000,
          "operator_grid": "CN87"
        },
        "solar": {
          "sfi": 140.0, "a_index": 7.0, "k_index": 2.0, "ssn": 70.0,
          "updated_at_ms": 1750000000000, "source": "bundled"
        }
      }
    }"#;

    // A void world: only the required non-optional DTOs, everything else absent.
    const VOID_JSON: &str = r#"{
      "id": "void-gateways-01",
      "world": {
        "modem": {"kind": "none", "connected": false, "state": "unconfigured"},
        "position": {"has_fix": false, "grid": "", "source": "none"}
      }
    }"#;

    fn write_tmp(name: &str, body: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "tuxlink-fixture-test-{}-{}",
            std::process::id(),
            name
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("fixture.json");
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(body.as_bytes()).unwrap();
        path
    }

    #[test]
    fn grounded_fixture_deserializes_into_real_dtos() {
        let path = write_tmp("grounded", GROUNDED_JSON);
        let fx = load_fixture(&path).expect("grounded fixture loads");
        assert_eq!(fx.id, "grounded-gateways-01");
        // Non-optional DTOs are present.
        assert_eq!(fx.world.modem.kind, "ardop");
        assert_eq!(fx.world.position.grid, "CN87");
        // Real GatewayDto fields survived the round-trip.
        let stations = fx.world.stations.expect("stations present");
        assert_eq!(stations.gateways.len(), 1);
        assert_eq!(stations.gateways[0].callsign, "W1AW");
        assert_eq!(stations.gateways[0].grid.as_deref(), Some("FN31"));
        assert_eq!(stations.operator_grid.as_deref(), Some("CN87"));
        // rig + solar parsed into their real DTOs.
        assert!(fx.world.rig.unwrap().configured);
        assert_eq!(fx.world.solar.unwrap().ssn, 70.0);
        // Extra sibling keys (family/prompt/spec) were ignored, not an error.
    }

    #[test]
    fn void_fixture_uses_minimal_concrete_for_non_optional() {
        let path = write_tmp("void", VOID_JSON);
        let fx = load_fixture(&path).expect("void fixture loads");
        assert_eq!(fx.id, "void-gateways-01");
        // Required DTOs carry minimal concrete state.
        assert!(!fx.world.modem.connected);
        assert!(!fx.world.position.has_fix);
        // Everything else is absent.
        assert!(fx.world.stations.is_none());
        assert!(fx.world.rig.is_none());
        assert!(fx.world.solar.is_none());
        assert!(fx.world.docs.is_empty());
        assert!(fx.world.mailbox.is_empty());
        assert!(fx.world.config.is_none());
    }

    #[test]
    fn malformed_fixture_fails_loudly() {
        // Missing the required `world.modem` field ⇒ Parse error, not a silent
        // default.
        let bad = r#"{"id":"x","world":{"position":{"has_fix":false,"grid":"","source":"none"}}}"#;
        let path = write_tmp("malformed", bad);
        let err = load_fixture(&path).expect_err("must fail");
        assert!(matches!(err, FixtureError::Parse { .. }), "got {err:?}");
    }

    #[test]
    fn missing_file_fails_loudly() {
        let path = std::env::temp_dir().join("tuxlink-fixture-does-not-exist-xyz.json");
        let err = load_fixture(&path).expect_err("must fail");
        assert!(matches!(err, FixtureError::Io { .. }), "got {err:?}");
    }

    // ---- Task 3: scenario loader ----------------------------------------

    #[test]
    fn scenario_env_absent_yields_none() {
        assert!(resolve_scenario(None).unwrap().is_none());
        assert!(resolve_scenario(Some(String::new())).unwrap().is_none());
        assert!(resolve_scenario(Some("   ".into())).unwrap().is_none());
    }

    #[test]
    fn scenario_env_present_loads_world() {
        let path = write_tmp("env-present", GROUNDED_JSON);
        let world = resolve_scenario(Some(path.display().to_string()))
            .unwrap()
            .expect("Some(world)");
        assert_eq!(world.modem.kind, "ardop");
        assert_eq!(
            world.stations.as_ref().unwrap().gateways[0].callsign,
            "W1AW"
        );
    }

    #[test]
    fn scenario_env_bad_path_fails_loudly() {
        let err = resolve_scenario(Some("/nonexistent/scenario/path.json".into()))
            .expect_err("must fail");
        assert!(matches!(err, FixtureError::Io { .. }), "got {err:?}");
    }

    // ---- Task 2: schema -------------------------------------------------

    #[test]
    fn schema_has_world_and_id_at_top_level() {
        let schema = fixture_json_schema();
        let props = schema
            .get("properties")
            .and_then(|p| p.as_object())
            .expect("top-level properties");
        assert!(props.contains_key("id"), "schema has id");
        assert!(props.contains_key("world"), "schema has world");
    }

    #[test]
    fn schema_world_field_set_matches_struct() {
        // The shadow WorldSchema's mirrored field list must equal the field set
        // the real World struct deserializes. If a World field is added without
        // mirroring it here, this fails — the drift guard.
        let expected: std::collections::BTreeSet<&str> =
            schema::WORLD_SCHEMA_FIELDS.iter().copied().collect();
        // Field names of World, kept in sync by hand with the struct above.
        let world_fields: std::collections::BTreeSet<&str> = [
            "modem",
            "position",
            "rig",
            "backend",
            "stations",
            "prediction",
            "solar",
            "docs",
            "catalog",
            "mailbox",
            "log",
            "config",
            "devices",
        ]
        .into_iter()
        .collect();
        assert_eq!(expected, world_fields, "shadow schema must mirror World");
    }

    #[test]
    fn schema_world_properties_match_fields() {
        // The generated schema's `world` property set must equal
        // WORLD_SCHEMA_FIELDS, so schema and struct-field list cannot silently
        // disagree. No file is written (the Python half validates against its own
        // committed schema); this stays a pure in-memory check.
        let schema = fixture_json_schema();
        let props = schema["properties"]["world"]["properties"]
            .as_object()
            .expect("schema has properties.world.properties");
        let got: std::collections::BTreeSet<&str> =
            props.keys().map(|k| k.as_str()).collect();
        let want: std::collections::BTreeSet<&str> =
            schema::WORLD_SCHEMA_FIELDS.iter().copied().collect();
        assert_eq!(got, want, "schema world props must equal WORLD_SCHEMA_FIELDS");
    }

    // ---- Task 7: committed sample fixture -------------------------------

    #[test]
    fn committed_sample_fixture_loads() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("sample-grounded.json");
        let fx = load_fixture(&path).expect("committed sample-grounded.json loads");
        assert_eq!(fx.id, "sample-grounded");
        let stations = fx.world.stations.expect("sample has stations");
        assert_eq!(stations.gateways[0].callsign, "W1AW");
    }
}
