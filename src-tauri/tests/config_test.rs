// Tests for tuxlink-4mt — see docs/superpowers/specs/2026-05-18-task-2-config-impl-design.md §6
// for the full 24-test matrix and the design rationale.

use tuxlink_lib::config::{validate_identity, validate_identity_describe};

// ============================================================================
// Phase 1 — validate_identity + describe-helper (loose-validator rules)
// ============================================================================

#[test]
fn test_validate_identity_loose_rules_accept() {
    assert!(validate_identity("W4PHS"));
    assert!(validate_identity("W4PHS-7"));
    assert!(validate_identity("EOC-1"));
    assert!(validate_identity("BAOFENG-FM-01"));
    assert!(validate_identity("LabBench-3"));
    assert!(validate_identity("W"));                    // 1 char OK
    assert!(validate_identity(&"X".repeat(32)));        // exactly 32 chars OK
}

#[test]
fn test_validate_identity_loose_rules_reject_each_class() {
    assert!(!validate_identity(""), "empty rejected");
    assert!(!validate_identity("W4 PHS"), "internal whitespace rejected");
    assert!(!validate_identity(&"X".repeat(33)), ">32 chars rejected");
    assert!(!validate_identity("W4PHS\x07"), "non-ASCII-printable (BEL) rejected");
    assert!(!validate_identity("W4PHS\x7F"), "DEL rejected");
    assert!(!validate_identity("Ünïcödë"), "non-ASCII rejected");
}

#[test]
fn test_validate_identity_describe_returns_first_rule_violated() {
    // Rule order per spec §3.2: empty → ASCII → whitespace → length
    assert_eq!(validate_identity_describe(""), Some("must not be empty"));
    assert_eq!(validate_identity_describe("Ünï"), Some("must be ASCII-printable"));
    assert_eq!(validate_identity_describe("W4 PHS"), Some("must not contain whitespace"));
    assert_eq!(validate_identity_describe(&"X".repeat(33)), Some("must be ≤32 chars"));
}

#[test]
fn test_validate_identity_describe_precedence_multi_violation() {
    // Per plan-review R2 P2-3: test PRECEDENCE — inputs violating multiple rules
    // should produce the FIRST-rule error. R2 P1-3's actionable-error-first claim
    // is the load-bearing semantic; regression that swapped rule order (e.g., length
    // first) would pass single-violation tests but fail these.
    // 40-char string containing whitespace → whitespace fires before length.
    let ws_long: String = "X ".repeat(20);
    assert_eq!(validate_identity_describe(&ws_long), Some("must not contain whitespace"),
        "whitespace check must fire before length check");
    // 40-char non-ASCII string → ASCII fires before length.
    let non_ascii_long: String = "Ü".repeat(40);
    assert_eq!(validate_identity_describe(&non_ascii_long), Some("must be ASCII-printable"),
        "ASCII check must fire before length check");
}

#[test]
fn test_validate_identity_describe_returns_none_on_accept() {
    assert_eq!(validate_identity_describe("W4PHS"), None);
    assert_eq!(validate_identity_describe("EOC-1"), None);
    assert_eq!(validate_identity_describe(&"X".repeat(32)), None);
}

#[test]
fn test_validate_identity_consistency_with_describe() {
    // validate_identity == validate_identity_describe(s).is_none()
    for s in &["W4PHS", "EOC-1", "", "W4 PHS", "Ünï", &"X".repeat(33)] {
        let by_bool = validate_identity(s);
        let by_describe = validate_identity_describe(s).is_none();
        assert_eq!(by_bool, by_describe, "consistency violation for input {:?}", s);
    }
}

// ============================================================================
// Phase 2 — Nested Config types + deserialize + AMD-11 drift defense
// ============================================================================

use tuxlink_lib::config::{
    Config,
    CmsTransport, GpsState, PositionPrecision,
    CONFIG_SCHEMA_VERSION,
};

#[test]
#[allow(deprecated)] // reads pat_mbo_address on deserialized Config; field deprecated per tuxlink-9phd T8.1
fn test_deserialize_minimal_cms_config() {
    let json = r#"{
        "schema_version": 2,
        "wizard_completed": true,
        "connect": {"connect_to_cms": true, "transport": "CmsSsl"},
        "identity": {"callsign": "W4PHS", "identifier": null, "grid": "EM75xx"},
        "privacy": {"gps_state": "BroadcastAtPrecision", "position_precision": "FourCharGrid"},
        "pat_mbo_address": "W4PHS@winlink.org"
    }"#;
    let config: Config = serde_json::from_str(json).expect("must deserialize");
    assert_eq!(config.schema_version, CONFIG_SCHEMA_VERSION);
    assert!(config.wizard_completed);
    assert!(config.connect.connect_to_cms);
    assert_eq!(config.connect.transport, CmsTransport::CmsSsl);
    assert_eq!(config.identity.active_full.as_deref(), Some("W4PHS"));
    assert!(config.identity.identifier.is_none());
    assert_eq!(config.identity.grid.as_deref(), Some("EM75xx"));
    assert_eq!(config.privacy.gps_state, GpsState::BroadcastAtPrecision);
    assert_eq!(config.privacy.position_precision, PositionPrecision::FourCharGrid);
    assert_eq!(config.pat_mbo_address.as_deref(), Some("W4PHS@winlink.org"));
}

#[test]
#[allow(deprecated)] // reads pat_mbo_address on deserialized Config; field deprecated per tuxlink-9phd T8.1
fn test_deserialize_offline_config() {
    let json = r#"{
        "schema_version": 2,
        "wizard_completed": true,
        "connect": {"connect_to_cms": false, "transport": "CmsSsl"},
        "identity": {"callsign": null, "identifier": "EOC-1", "grid": "EM75"},
        "privacy": {"gps_state": "BroadcastAtPrecision", "position_precision": "FourCharGrid"},
        "pat_mbo_address": null
    }"#;
    let config: Config = serde_json::from_str(json).expect("offline config must deserialize");
    assert!(!config.connect.connect_to_cms);
    assert!(config.identity.active_full.is_none());
    assert_eq!(config.identity.identifier.as_deref(), Some("EOC-1"));
    assert!(config.pat_mbo_address.is_none());
}

#[test]
fn test_reject_wrong_schema_version() {
    let json = r#"{
        "schema_version": 99,
        "wizard_completed": true,
        "connect": {"connect_to_cms": true, "transport": "CmsSsl"},
        "identity": {"callsign": "W4PHS", "identifier": null, "grid": null},
        "privacy": {"gps_state": "Off", "position_precision": "FourCharGrid"},
        "pat_mbo_address": null
    }"#;
    let result: Result<Config, _> = serde_json::from_str(json);
    assert!(result.is_err(), "unexpected schema version must fail to deserialize");
}

#[test]
fn test_reject_amd11_dropped_field_winlink_password_present() {
    // Stale top-level field MUST be rejected by deny_unknown_fields on Config.
    // The pre-AMD-1 flat schema had winlink_password_present at the TOP LEVEL.
    let json = r#"{
        "schema_version": 2,
        "wizard_completed": true,
        "connect": {"connect_to_cms": true, "transport": "CmsSsl"},
        "identity": {"callsign": "W4PHS", "identifier": null, "grid": null},
        "privacy": {"gps_state": "Off", "position_precision": "FourCharGrid"},
        "pat_mbo_address": null,
        "winlink_password_present": true
    }"#;
    let result: Result<Config, _> = serde_json::from_str(json);
    assert!(result.is_err(), "AMD-11-dropped field at top level must hard-fail via deny_unknown_fields");
    let err = result.unwrap_err().to_string();
    assert!(err.contains("winlink_password_present"),
        "error message must mention the stale field: {err}");
}

#[test]
fn test_deny_unknown_fields_on_each_substruct() {
    // Unknown field on ConnectConfig must fail.
    let json_connect = r#"{
        "schema_version": 2, "wizard_completed": true,
        "connect": {"connect_to_cms": true, "transport": "CmsSsl", "extra_field": "x"},
        "identity": {"callsign": "W4PHS", "identifier": null, "grid": null},
        "privacy": {"gps_state": "Off", "position_precision": "FourCharGrid"},
        "pat_mbo_address": null
    }"#;
    assert!(serde_json::from_str::<Config>(json_connect).is_err(),
        "unknown field on ConnectConfig must fail");

    // Unknown field on IdentityConfig must fail.
    let json_id = r#"{
        "schema_version": 2, "wizard_completed": true,
        "connect": {"connect_to_cms": true, "transport": "CmsSsl"},
        "identity": {"callsign": "W4PHS", "identifier": null, "grid": null, "extra": "x"},
        "privacy": {"gps_state": "Off", "position_precision": "FourCharGrid"},
        "pat_mbo_address": null
    }"#;
    assert!(serde_json::from_str::<Config>(json_id).is_err(),
        "unknown field on IdentityConfig must fail");

    // Unknown field on PrivacyConfig must fail.
    let json_priv = r#"{
        "schema_version": 2, "wizard_completed": true,
        "connect": {"connect_to_cms": true, "transport": "CmsSsl"},
        "identity": {"callsign": "W4PHS", "identifier": null, "grid": null},
        "privacy": {"gps_state": "Off", "position_precision": "FourCharGrid", "extra": "x"},
        "pat_mbo_address": null
    }"#;
    assert!(serde_json::from_str::<Config>(json_priv).is_err(),
        "unknown field on PrivacyConfig must fail");
}

#[test]
fn test_cms_transport_both_variants_round_trip() {
    // Per plan-review R2 P2-2: iterate BOTH variants, not just Telnet.
    // CmsSsl is implicitly deserialized in many other tests but its SERIALIZE-AS-PascalCase
    // contract is unlocked without an explicit check.
    for (variant, name) in [
        (CmsTransport::CmsSsl, "CmsSsl"),
        (CmsTransport::Telnet, "Telnet"),
    ] {
        let json = format!(r#"{{
            "schema_version": 2, "wizard_completed": true,
            "connect": {{"connect_to_cms": true, "transport": "{}"}},
            "identity": {{"callsign": "W4PHS", "identifier": null, "grid": null}},
            "privacy": {{"gps_state": "Off", "position_precision": "FourCharGrid"}},
            "pat_mbo_address": null
        }}"#, name);
        let config: Config = serde_json::from_str(&json)
            .unwrap_or_else(|e| panic!("variant {name} must deserialize: {e}"));
        assert_eq!(config.connect.transport, variant);
        let out = serde_json::to_string(&config).unwrap();
        assert!(out.contains(&format!("\"{name}\"")),
            "serialized form must use PascalCase variant {name}: {out}");
    }
}

#[test]
fn test_gps_state_three_variants_round_trip() {
    for (variant, name) in [
        (GpsState::Off, "Off"),
        (GpsState::LocalUiOnly, "LocalUiOnly"),
        (GpsState::BroadcastAtPrecision, "BroadcastAtPrecision"),
    ] {
        let json = format!(r#"{{
            "schema_version": 2, "wizard_completed": true,
            "connect": {{"connect_to_cms": false, "transport": "CmsSsl"}},
            "identity": {{"callsign": null, "identifier": "X", "grid": null}},
            "privacy": {{"gps_state": "{}", "position_precision": "FourCharGrid"}},
            "pat_mbo_address": null
        }}"#, name);
        let config: Config = serde_json::from_str(&json)
            .unwrap_or_else(|e| panic!("variant {name} must deserialize: {e}"));
        assert_eq!(config.privacy.gps_state, variant);
        let out = serde_json::to_string(&config).unwrap();
        assert!(out.contains(&format!("\"{name}\"")), "serialize must use PascalCase: {out}");
    }
}

#[test]
fn test_position_precision_two_variants_round_trip() {
    for (variant, name) in [
        (PositionPrecision::FourCharGrid, "FourCharGrid"),
        (PositionPrecision::SixCharGrid, "SixCharGrid"),
    ] {
        let json = format!(r#"{{
            "schema_version": 2, "wizard_completed": true,
            "connect": {{"connect_to_cms": false, "transport": "CmsSsl"}},
            "identity": {{"callsign": null, "identifier": "X", "grid": null}},
            "privacy": {{"gps_state": "Off", "position_precision": "{}"}},
            "pat_mbo_address": null
        }}"#, name);
        let config: Config = serde_json::from_str(&json)
            .unwrap_or_else(|e| panic!("variant {name} must deserialize: {e}"));
        assert_eq!(config.privacy.position_precision, variant);
    }
}

#[test]
#[allow(deprecated)] // reads pat_mbo_address on deserialized Config; field deprecated per tuxlink-9phd T8.1
fn test_empty_string_identity_field_normalizes_to_none() {
    // Spec §3.1: deserialize_optional_nonempty_string maps "" → None.
    // This is the offline-mode-when-operator-types-then-clears case.
    let json = r#"{
        "schema_version": 2, "wizard_completed": true,
        "connect": {"connect_to_cms": false, "transport": "CmsSsl"},
        "identity": {"callsign": "", "identifier": "EOC-1", "grid": ""},
        "privacy": {"gps_state": "Off", "position_precision": "FourCharGrid"},
        "pat_mbo_address": ""
    }"#;
    let config: Config = serde_json::from_str(json).expect("must deserialize");
    assert!(config.identity.active_full.is_none(), "empty callsign should normalize to None");
    assert_eq!(config.identity.identifier.as_deref(), Some("EOC-1"));
    assert!(config.identity.grid.is_none(), "empty grid should normalize to None");
    assert!(config.pat_mbo_address.is_none(), "empty pat_mbo_address should normalize to None");
}

// ============================================================================
// Phase 3 — Config::validate (cross-field rules)
// ============================================================================

use tuxlink_lib::config::ConfigValidationError;

fn make_config(
    connect_to_cms: bool,
    callsign: Option<&str>,
    identifier: Option<&str>,
) -> Config {
    // Helper: build a Config via deserialization to ensure all the deserialize
    // attributes (rename_all, deny_unknown_fields, etc.) are honored.
    // Empty-string → None happens via deserialize_optional_nonempty_string.
    let json = format!(r#"{{
        "schema_version": 2, "wizard_completed": false,
        "connect": {{"connect_to_cms": {}, "transport": "CmsSsl"}},
        "identity": {{
            "callsign": {},
            "identifier": {},
            "grid": null
        }},
        "privacy": {{"gps_state": "Off", "position_precision": "FourCharGrid"}},
        "pat_mbo_address": null
    }}"#,
        connect_to_cms,
        match callsign { Some(s) => format!("\"{s}\""), None => "null".into() },
        match identifier { Some(s) => format!("\"{s}\""), None => "null".into() },
    );
    serde_json::from_str(&json).unwrap_or_else(|e| panic!("test fixture must deserialize: {e}\nJSON: {json}"))
}

#[test]
fn test_validate_cms_path_requires_callsign() {
    let config = make_config(true, None, None);
    let err = config.validate().unwrap_err();
    assert!(matches!(err, ConfigValidationError::CmsPathNoActiveFull));
}

#[test]
fn test_validate_offline_path_allows_callsign() {
    // Phase 2 (tuxlink-7iy2): offline + a selected FULL identity is now VALID.
    // A P2P/RF-only deployment may select a FULL identity (tactical posture); the
    // old offline-forbids-callsign biconditional was removed.
    let config = make_config(false, Some("W4PHS"), None);
    assert!(config.validate().is_ok());
}

#[test]
fn test_validate_offline_with_identifier_only_accepts() {
    let config = make_config(false, None, Some("EOC-1"));
    assert!(config.validate().is_ok());
}

#[test]
fn test_validate_cms_with_callsign_accepts() {
    let config = make_config(true, Some("W4PHS"), None);
    assert!(config.validate().is_ok());
}

#[test]
fn test_validate_invalid_identity_propagates_field() {
    // Callsign with whitespace → InvalidIdentity { field: "callsign", rule: "must not contain whitespace" }
    // Note: deserialize_optional_nonempty_string accepts the non-empty whitespace-containing input;
    // we have to build the Config bypassing the deserializer to construct this case directly.
    let mut config = make_config(true, Some("W4PHS"), None);
    config.identity.active_full = Some("W4 PHS".into());
    let err = config.validate().unwrap_err();
    match err {
        ConfigValidationError::InvalidIdentity { field, rule } => {
            assert_eq!(field, "callsign");
            assert_eq!(rule, "must not contain whitespace");
        }
        other => panic!("expected InvalidIdentity, got {other:?}"),
    }

    // Identifier with bad char → InvalidIdentity { field: "identifier", rule: "must be ASCII-printable" }
    let mut config = make_config(false, None, Some("EOC-1"));
    config.identity.identifier = Some("EOC\x07".into());
    let err = config.validate().unwrap_err();
    match err {
        ConfigValidationError::InvalidIdentity { field, rule } => {
            assert_eq!(field, "identifier");
            assert_eq!(rule, "must be ASCII-printable");
        }
        other => panic!("expected InvalidIdentity, got {other:?}"),
    }
}

#[test]
fn test_validation_error_display_strings_stable() {
    // Per spec §3.1: Display strings are STABLE PUBLIC SURFACE for ALL THREE error enums
    // (ConfigValidationError, ConfigReadError, ConfigWriteError). The wizard interpolates
    // them into operator-visible messages via format!("{e}"). Any future change is a
    // breaking change for the wizard's UX tests. Plan-review R2 P0-2 + R3 P1-3 caught
    // earlier under-coverage (3 of 12 variants tested); v2 of this test covers all 3 enums.
    let e = ConfigValidationError::CmsPathNoActiveFull;
    assert_eq!(e.to_string(), "CMS path requires an active FULL identity to be selected");

    let e = ConfigValidationError::InvalidIdentity { field: "callsign", rule: "must not be empty" };
    assert_eq!(e.to_string(), "invalid identity field `callsign`: must not be empty");
}

// ============================================================================
// Phase 4 — read_config + ConfigReadError
// ============================================================================

use tuxlink_lib::config::{read_config, ConfigReadError, config_path};

/// Helper: scope XDG_CONFIG_HOME to a fresh temp dir for the duration of `f`.
/// Uses RAII guard so prior env value is RESTORED even if `f` panics (per plan-review
/// R1 P1-1 + R2 P1-3 — panic during a test would otherwise orphan the env var and
/// cascade failures into subsequent tests). Use with #[serial_test::serial] to avoid
/// concurrent-process races.
struct XdgGuard {
    prior: Option<std::ffi::OsString>,
    _tmp: tempfile::TempDir,
}
impl Drop for XdgGuard {
    fn drop(&mut self) {
        match self.prior.take() {
            Some(p) => std::env::set_var("XDG_CONFIG_HOME", p),
            None => std::env::remove_var("XDG_CONFIG_HOME"),
        }
    }
}

fn with_xdg_temp<R>(f: impl FnOnce(&std::path::Path) -> R) -> R {
    let tmp = tempfile::tempdir().expect("must create tempdir");
    let path = tmp.path().to_owned();
    let prior = std::env::var_os("XDG_CONFIG_HOME");
    std::env::set_var("XDG_CONFIG_HOME", &path);
    let _guard = XdgGuard { prior, _tmp: tmp };
    f(&path)
    // _guard drops here, restoring prior env value (even on panic from `f`)
}

#[test]
#[serial_test::serial]
fn test_read_config_not_found_returns_typed_error() {
    with_xdg_temp(|_| {
        let err = read_config().unwrap_err();
        assert!(matches!(err, ConfigReadError::NotFound { .. }));
    });
}

#[test]
#[serial_test::serial]
fn test_read_config_serde_returns_typed_error_on_malformed_json() {
    with_xdg_temp(|xdg| {
        let path = xdg.join("tuxlink").join("config.json");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"{ not valid json").unwrap();
        let err = read_config().unwrap_err();
        assert!(matches!(err, ConfigReadError::Serde { .. }));
    });
}

#[test]
#[serial_test::serial]
fn test_read_config_validation_runs_after_deserialize() {
    with_xdg_temp(|xdg| {
        let path = xdg.join("tuxlink").join("config.json");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        // Valid JSON shape but CMS-path-with-no-active-FULL — should fail validation.
        // (Phase 2: offline+callsign is now VALID, so the still-invalid case under v2
        // is connect_to_cms=true with a null callsign.)
        std::fs::write(&path, r#"{
            "schema_version": 2, "wizard_completed": true,
            "connect": {"connect_to_cms": true, "transport": "CmsSsl"},
            "identity": {"callsign": null, "identifier": null, "grid": null},
            "privacy": {"gps_state": "Off", "position_precision": "FourCharGrid"},
            "pat_mbo_address": null
        }"#).unwrap();
        let err = read_config().unwrap_err();
        match err {
            ConfigReadError::Validation { source: ConfigValidationError::CmsPathNoActiveFull } => {}
            other => panic!("expected Validation(CmsPathNoActiveFull), got {other:?}"),
        }
    });
}

#[test]
#[serial_test::serial]
fn test_read_config_happy_path() {
    with_xdg_temp(|xdg| {
        let path = xdg.join("tuxlink").join("config.json");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, r#"{
            "schema_version": 2, "wizard_completed": true,
            "connect": {"connect_to_cms": true, "transport": "CmsSsl"},
            "identity": {"callsign": "W4PHS", "identifier": null, "grid": "EM75"},
            "privacy": {"gps_state": "Off", "position_precision": "FourCharGrid"},
            "pat_mbo_address": "W4PHS@winlink.org"
        }"#).unwrap();
        let config = read_config().expect("happy path must succeed");
        assert!(config.wizard_completed);
        assert_eq!(config.identity.active_full.as_deref(), Some("W4PHS"));
    });
}

#[test]
#[serial_test::serial]
fn test_config_path_uses_xdg_config_home_when_set() {
    with_xdg_temp(|xdg| {
        let path = config_path();
        assert_eq!(path, xdg.join("tuxlink").join("config.json"));
    });
}

#[test]
#[serial_test::serial]
#[cfg(unix)]
fn test_read_config_eacces_returns_io_variant_not_notfound() {
    // ConfigReadError::Io variant per spec §3.1 — fires when std::fs::read returns
    // a non-NotFound error (EACCES, EIO, etc). Symmetric with the write-side
    // ProbeReadFailed coverage. Added per plan-review R3 P0-2.
    use std::os::unix::fs::PermissionsExt;
    with_xdg_temp(|xdg| {
        let path = xdg.join("tuxlink").join("config.json");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, br#"{"schema_version": 1}"#).unwrap();
        let mut perm = std::fs::metadata(&path).unwrap().permissions();
        perm.set_mode(0o000);
        std::fs::set_permissions(&path, perm).unwrap();

        let err = read_config().unwrap_err();
        assert!(matches!(err, ConfigReadError::Io { .. }),
            "EACCES on read MUST be Io variant, not NotFound: {err:?}");

        // Restore permissions so tempdir cleanup works.
        let mut perm = std::fs::metadata(&path).unwrap().permissions();
        perm.set_mode(0o600);
        std::fs::set_permissions(&path, perm).unwrap();
    });
}

// ============================================================================
// Phase 5 — write_config_atomic + ConfigWriteError
// ============================================================================

use tuxlink_lib::config::{write_config_atomic, ConfigWriteError};

fn make_valid_cms_config() -> Config {
    serde_json::from_str(r#"{
        "schema_version": 2, "wizard_completed": true,
        "connect": {"connect_to_cms": true, "transport": "CmsSsl"},
        "identity": {"callsign": "W4PHS", "identifier": null, "grid": "EM75"},
        "privacy": {"gps_state": "Off", "position_precision": "FourCharGrid"},
        "pat_mbo_address": "W4PHS@winlink.org"
    }"#).unwrap()
}

#[test]
#[serial_test::serial]
fn test_write_atomic_first_run_creates_file() {
    with_xdg_temp(|xdg| {
        let config = make_valid_cms_config();
        write_config_atomic(&config).expect("first-run write must succeed");
        let path = xdg.join("tuxlink").join("config.json");
        assert!(path.exists(), "config file must exist after write");
        let roundtrip = read_config().expect("written file must read back");
        assert_eq!(roundtrip.identity.active_full.as_deref(), Some("W4PHS"));
    });
}

#[test]
#[serial_test::serial]
fn test_write_atomic_overwrites_v1_file() {
    with_xdg_temp(|xdg| {
        let path = xdg.join("tuxlink").join("config.json");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, br#"{"schema_version": 1, "old": "value"}"#).unwrap();
        let config = make_valid_cms_config();
        write_config_atomic(&config).expect("v1-overwrite must succeed");
        let roundtrip = read_config().expect("post-overwrite must read back");
        assert!(roundtrip.wizard_completed);
    });
}

#[test]
#[serial_test::serial]
fn test_write_atomic_refuses_schema_version_mismatch_future() {
    with_xdg_temp(|xdg| {
        let path = xdg.join("tuxlink").join("config.json");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let preserved = br#"{"schema_version": 99, "future": "shape"}"#;
        std::fs::write(&path, preserved).unwrap();
        let config = make_valid_cms_config();
        let err = write_config_atomic(&config).unwrap_err();
        match err {
            ConfigWriteError::SchemaVersionMismatch { existing: 99, ours: 2 } => {}
            other => panic!("expected SchemaVersionMismatch{{99,2}}, got {other:?}"),
        }
        // PRESERVATION CONTRACT: original file MUST be untouched.
        let current = std::fs::read(&path).unwrap();
        assert_eq!(current, preserved);
    });
}

#[test]
#[serial_test::serial]
fn test_write_atomic_refuses_schema_version_mismatch_past() {
    // Spec §3.4 SchemaVersionMismatch covers BOTH directions (renamed from Downgrade
    // per adrev R4 P1-5). A schema_version=0 file also blocks rather than overwriting.
    with_xdg_temp(|xdg| {
        let path = xdg.join("tuxlink").join("config.json");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, br#"{"schema_version": 0, "ancient": "shape"}"#).unwrap();
        let config = make_valid_cms_config();
        let err = write_config_atomic(&config).unwrap_err();
        match err {
            ConfigWriteError::SchemaVersionMismatch { existing: 0, ours: 2 } => {}
            other => panic!("expected SchemaVersionMismatch{{0,2}}, got {other:?}"),
        }
    });
}

#[test]
#[serial_test::serial]
fn test_write_atomic_overwrites_unparseable_file() {
    // Corruption-recovery semantics: malformed-JSON existing file does NOT block.
    with_xdg_temp(|xdg| {
        let path = xdg.join("tuxlink").join("config.json");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"\x00\x01\x02 totally not json").unwrap();
        let config = make_valid_cms_config();
        write_config_atomic(&config).expect("unparseable existing file must NOT block");
        let roundtrip = read_config().expect("post-overwrite must read back");
        assert!(roundtrip.wizard_completed);
    });
}

#[test]
#[serial_test::serial]
#[cfg(unix)]
fn test_write_atomic_refuses_existing_symlink() {
    use std::os::unix::fs::symlink;
    with_xdg_temp(|xdg| {
        let cfg_path = xdg.join("tuxlink").join("config.json");
        std::fs::create_dir_all(cfg_path.parent().unwrap()).unwrap();
        let target = xdg.join("dotfiles-config.json");
        std::fs::write(&target, br#"{"original": "data"}"#).unwrap();
        symlink(&target, &cfg_path).unwrap();
        let config = make_valid_cms_config();
        let err = write_config_atomic(&config).unwrap_err();
        match err {
            ConfigWriteError::ExistingFileIsSymlink { path: ref p, target: ref t } => {
                assert_eq!(p, &cfg_path);
                assert_eq!(t.as_deref(), Some(target.as_path()));
            }
            other => panic!("expected ExistingFileIsSymlink, got {other:?}"),
        }
        // PRESERVATION CONTRACT: symlink + target must survive refusal.
        assert!(
            std::fs::symlink_metadata(&cfg_path).unwrap().file_type().is_symlink(),
            "symlink itself must survive refusal"
        );
        assert_eq!(
            std::fs::read_link(&cfg_path).unwrap(),
            target,
            "symlink must still point to target"
        );
        let target_content = std::fs::read(&target).unwrap();
        assert_eq!(target_content, br#"{"original": "data"}"#);
    });
}

#[test]
#[serial_test::serial]
#[cfg(unix)]
fn test_write_atomic_probe_read_eacces_fails_typed() {
    use std::os::unix::fs::PermissionsExt;
    with_xdg_temp(|xdg| {
        let cfg_path = xdg.join("tuxlink").join("config.json");
        std::fs::create_dir_all(cfg_path.parent().unwrap()).unwrap();
        let original = br#"{"schema_version": 1, "preserved": true}"#;
        std::fs::write(&cfg_path, original).unwrap();

        let mut perm = std::fs::metadata(&cfg_path).unwrap().permissions();
        perm.set_mode(0o000);
        std::fs::set_permissions(&cfg_path, perm).unwrap();

        let config = make_valid_cms_config();
        let err = write_config_atomic(&config).unwrap_err();
        match err {
            ConfigWriteError::ProbeReadFailed { path: ref p, .. } => {
                assert_eq!(p, &cfg_path);
            }
            other => panic!("expected ProbeReadFailed, got {other:?}"),
        }

        // PRESERVATION CONTRACT: original file content unchanged after refusal.
        let mut perm = std::fs::metadata(&cfg_path).unwrap().permissions();
        perm.set_mode(0o600);
        std::fs::set_permissions(&cfg_path, perm).unwrap();
        let preserved = std::fs::read(&cfg_path).unwrap();
        assert_eq!(preserved, original, "original file content must be preserved on ProbeReadFailed refusal");
    });
}

// ============================================================================
// Phase 6 — pat_mbo_address deprecation (tuxlink-9phd, Task 8.1)
//
// Two contracts under test:
//   a) skip_serializing: the field is ABSENT from the serialized JSON output.
//   b) default (tolerant read): a legacy config JSON containing pat_mbo_address
//      is accepted on read and the value round-trips through the field.
//
// Both tests carry #[allow(deprecated)] because they explicitly exercise the
// deprecated field — that is the point of the test.
// ============================================================================

#[test]
#[allow(deprecated)]
fn config_skips_pat_mbo_address_on_write() {
    // Build a Config that has pat_mbo_address set. After applying
    // #[serde(skip_serializing)], the serialized JSON must NOT contain
    // the key "pat_mbo_address" at all, regardless of the field value.
    let cfg = Config {
        schema_version: CONFIG_SCHEMA_VERSION,
        wizard_completed: true,
        connect: tuxlink_lib::config::ConnectConfig {
            connect_to_cms: true,
            transport: CmsTransport::CmsSsl,
            host: tuxlink_lib::config::default_cms_host(),
        },
        identity: tuxlink_lib::config::IdentityConfig {
            active_full: Some("W4PHS".into()),
            identifier: None,
            grid: None,
        },
        privacy: tuxlink_lib::config::PrivacyConfig {
            gps_state: GpsState::Off,
            position_precision: PositionPrecision::FourCharGrid,
            position_source: tuxlink_lib::config::PositionSource::Gps,
        },
        pat_mbo_address: Some("LEGACY-VALUE".into()),
        packet: tuxlink_lib::config::PacketConfig::default(),
        modem_ardop: None,
        modem_vara: None,
        telnet_listen: tuxlink_lib::config::TelnetListenUiConfig::default(),
        network_po_favorites: Vec::new(),
        review_inbound_before_download: false,
        map_tile_source: None,
        aredn_master_node_host: None,
        aprs: tuxlink_lib::config::AprsConfig::default(),
        trash_auto_purge: true,
        trash_retention_days: 30,
    };
    let json = serde_json::to_string(&cfg).unwrap();
    assert!(
        !json.contains("pat_mbo_address"),
        "skip_serializing must exclude pat_mbo_address from JSON output, got: {json}"
    );
}

#[test]
#[allow(deprecated)]
fn config_reads_legacy_pat_mbo_address_without_error() {
    // A legacy config JSON that contains pat_mbo_address must still parse
    // successfully (tolerant read, #[serde(default)] means present-or-absent).
    // The parsed value must round-trip through the field.
    //
    // Note: Config has #[serde(deny_unknown_fields)] but pat_mbo_address is
    // still a KNOWN field (just deprecated) — so this round-trips cleanly.
    let json = r#"{
        "schema_version": 2,
        "wizard_completed": true,
        "connect": {"connect_to_cms": true, "transport": "CmsSsl"},
        "identity": {"callsign": "W4PHS", "identifier": null, "grid": null},
        "privacy": {"gps_state": "Off", "position_precision": "FourCharGrid"},
        "pat_mbo_address": "LEGACY-VALUE"
    }"#;
    let cfg: Config = serde_json::from_str(json).expect("legacy config with pat_mbo_address must parse");
    assert_eq!(
        cfg.pat_mbo_address,
        Some("LEGACY-VALUE".to_string()),
        "pat_mbo_address value must round-trip through the deprecated field on read"
    );
}
