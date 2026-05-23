// Tests for tuxlink-756 — pat_config render at PatProcess spawn.
//
// Spec: docs/superpowers/specs/2026-05-19-pat-config-render-design.md §4
// bd issue: tuxlink-756
//
// 6 tests: 1 happy path + 2 input-error paths + 2 I/O paths + 1 overwrite property.

use serde_json::Value;
use tuxlink_lib::config::{
    CmsTransport, Config, ConnectConfig, GpsState, IdentityConfig, PacketConfig, PositionPrecision,
    PrivacyConfig,
};
use tuxlink_lib::pat_config::{
    render_pat_config, write_pat_config_atomic, PatConfigError, PAT_CONFIG_SCHEMA_FIELDS,
};

// Helper: build a minimal valid CMS-path config.
fn cms_config() -> Config {
    Config {
        schema_version: 1,
        wizard_completed: true,
        connect: ConnectConfig {
            connect_to_cms: true,
            transport: CmsTransport::CmsSsl,
            host: tuxlink_lib::config::default_cms_host(),
        },
        identity: IdentityConfig {
            callsign: Some("W4PHS".to_string()),
            identifier: None,
            grid: Some("FM18lu".to_string()),
        },
        privacy: PrivacyConfig {
            gps_state: GpsState::Off,
            position_precision: PositionPrecision::FourCharGrid,
            position_source: tuxlink_lib::config::PositionSource::Gps,
        },
        pat_mbo_address: Some("W4PHS@winlink.org".to_string()),
        packet: PacketConfig::default(),
    }
}

// ============================================================================
// Test 1: happy path — minimal CMS config renders all expected fields
// ============================================================================
#[test]
fn test_render_pat_config_emits_expected_fields_for_minimal_cms_config() {
    let cfg = cms_config();
    let json_str = render_pat_config(&cfg).expect("render");
    let json: Value = serde_json::from_str(&json_str).expect("valid JSON");

    // Every PAT_CONFIG_SCHEMA_FIELDS key must be present in the rendered output.
    for field in PAT_CONFIG_SCHEMA_FIELDS {
        assert!(
            json.get(field).is_some(),
            "rendered Pat config missing expected field: {field}"
        );
    }

    assert_eq!(json["mycall"], "W4PHS");
    assert_eq!(json["locator"], "FM18lu");
    assert_eq!(json["auxiliary_addresses"], serde_json::json!([]));
    assert_eq!(json["auto_download_size_limit"], -1);
    assert_eq!(json["service_codes"], serde_json::json!(["PUBLIC"]));
    assert_eq!(json["http_addr"], "");
}

// ============================================================================
// Test 2: empty grid → empty locator string (Pat tolerates empty Locator)
// ============================================================================
#[test]
fn test_render_pat_config_with_empty_grid_emits_empty_locator() {
    let mut cfg = cms_config();
    cfg.identity.grid = None;
    let json_str = render_pat_config(&cfg).expect("render");
    let json: Value = serde_json::from_str(&json_str).expect("valid JSON");
    assert_eq!(json["locator"], "");
    assert_eq!(json["mycall"], "W4PHS");
}

// ============================================================================
// Test 3: missing callsign on CMS path → MissingRequiredField error
// ============================================================================
#[test]
fn test_render_pat_config_missing_callsign_returns_missing_required_field() {
    let mut cfg = cms_config();
    cfg.identity.callsign = None;
    match render_pat_config(&cfg).unwrap_err() {
        PatConfigError::MissingRequiredField(field) => {
            assert!(
                field.contains("callsign"),
                "expected error to name the missing field, got: {field}"
            );
        }
        other => panic!("expected MissingRequiredField, got {:?}", other),
    }
}

// ============================================================================
// Test 4: offline-mode config → OfflineModeNoConfigNeeded error
// ============================================================================
#[test]
fn test_render_pat_config_offline_mode_returns_offline_mode_error() {
    let mut cfg = cms_config();
    cfg.connect.connect_to_cms = false;
    cfg.identity.callsign = None;
    cfg.identity.identifier = Some("EOC-1".to_string());
    match render_pat_config(&cfg).unwrap_err() {
        PatConfigError::OfflineModeNoConfigNeeded => {}
        other => panic!("expected OfflineModeNoConfigNeeded, got {:?}", other),
    }
}

// ============================================================================
// Test 5: write_pat_config_atomic creates parent dir + writes file
// ============================================================================
#[test]
fn test_write_pat_config_atomic_creates_parent_dir_and_writes_file() {
    let tmp = tempfile::tempdir().expect("tmpdir");
    let dest = tmp.path().join("nested/pat/config.json");
    assert!(!dest.parent().unwrap().exists(), "precondition: parent dir absent");

    let cfg = cms_config();
    write_pat_config_atomic(&cfg, &dest).expect("write");

    assert!(dest.parent().unwrap().exists(), "parent dir created");
    assert!(dest.exists(), "config file created");

    let on_disk = std::fs::read_to_string(&dest).expect("read");
    let expected = render_pat_config(&cfg).expect("render");
    assert_eq!(
        on_disk, expected,
        "file content must equal what render_pat_config returned"
    );
}

// ============================================================================
// Test 6: write_pat_config_atomic overwrites an existing file atomically
// ============================================================================
#[test]
fn test_write_pat_config_atomic_overwrites_existing_file() {
    let tmp = tempfile::tempdir().expect("tmpdir");
    let dest = tmp.path().join("pat-config.json");
    std::fs::write(&dest, b"PRE-EXISTING BOGUS CONTENT").expect("seed file");

    let cfg = cms_config();
    write_pat_config_atomic(&cfg, &dest).expect("write");

    let on_disk = std::fs::read_to_string(&dest).expect("read");
    let expected = render_pat_config(&cfg).expect("render");
    assert_eq!(on_disk, expected, "overwrite must replace bogus content");
    assert!(
        !on_disk.contains("BOGUS"),
        "pre-existing content must be gone"
    );
}
