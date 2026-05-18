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
    let ws_long: String = std::iter::repeat("X ").take(20).collect();
    assert_eq!(validate_identity_describe(&ws_long), Some("must not contain whitespace"),
        "whitespace check must fire before length check");
    // 40-char non-ASCII string → ASCII fires before length.
    let non_ascii_long: String = std::iter::repeat("Ü").take(40).collect();
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
fn test_deserialize_minimal_cms_config() {
    let json = r#"{
        "schema_version": 1,
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
    assert_eq!(config.identity.callsign.as_deref(), Some("W4PHS"));
    assert!(config.identity.identifier.is_none());
    assert_eq!(config.identity.grid.as_deref(), Some("EM75xx"));
    assert_eq!(config.privacy.gps_state, GpsState::BroadcastAtPrecision);
    assert_eq!(config.privacy.position_precision, PositionPrecision::FourCharGrid);
    assert_eq!(config.pat_mbo_address.as_deref(), Some("W4PHS@winlink.org"));
}

#[test]
fn test_deserialize_offline_config() {
    let json = r#"{
        "schema_version": 1,
        "wizard_completed": true,
        "connect": {"connect_to_cms": false, "transport": "CmsSsl"},
        "identity": {"callsign": null, "identifier": "EOC-1", "grid": "EM75"},
        "privacy": {"gps_state": "BroadcastAtPrecision", "position_precision": "FourCharGrid"},
        "pat_mbo_address": null
    }"#;
    let config: Config = serde_json::from_str(json).expect("offline config must deserialize");
    assert!(!config.connect.connect_to_cms);
    assert!(config.identity.callsign.is_none());
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
        "schema_version": 1,
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
        "schema_version": 1, "wizard_completed": true,
        "connect": {"connect_to_cms": true, "transport": "CmsSsl", "extra_field": "x"},
        "identity": {"callsign": "W4PHS", "identifier": null, "grid": null},
        "privacy": {"gps_state": "Off", "position_precision": "FourCharGrid"},
        "pat_mbo_address": null
    }"#;
    assert!(serde_json::from_str::<Config>(json_connect).is_err(),
        "unknown field on ConnectConfig must fail");

    // Unknown field on IdentityConfig must fail.
    let json_id = r#"{
        "schema_version": 1, "wizard_completed": true,
        "connect": {"connect_to_cms": true, "transport": "CmsSsl"},
        "identity": {"callsign": "W4PHS", "identifier": null, "grid": null, "extra": "x"},
        "privacy": {"gps_state": "Off", "position_precision": "FourCharGrid"},
        "pat_mbo_address": null
    }"#;
    assert!(serde_json::from_str::<Config>(json_id).is_err(),
        "unknown field on IdentityConfig must fail");

    // Unknown field on PrivacyConfig must fail.
    let json_priv = r#"{
        "schema_version": 1, "wizard_completed": true,
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
            "schema_version": 1, "wizard_completed": true,
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
            "schema_version": 1, "wizard_completed": true,
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
            "schema_version": 1, "wizard_completed": true,
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
fn test_empty_string_identity_field_normalizes_to_none() {
    // Spec §3.1: deserialize_optional_nonempty_string maps "" → None.
    // This is the offline-mode-when-operator-types-then-clears case.
    let json = r#"{
        "schema_version": 1, "wizard_completed": true,
        "connect": {"connect_to_cms": false, "transport": "CmsSsl"},
        "identity": {"callsign": "", "identifier": "EOC-1", "grid": ""},
        "privacy": {"gps_state": "Off", "position_precision": "FourCharGrid"},
        "pat_mbo_address": ""
    }"#;
    let config: Config = serde_json::from_str(json).expect("must deserialize");
    assert!(config.identity.callsign.is_none(), "empty callsign should normalize to None");
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
        "schema_version": 1, "wizard_completed": false,
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
    assert!(matches!(err, ConfigValidationError::CmsPathMissingCallsign));
}

#[test]
fn test_validate_offline_path_rejects_callsign() {
    let config = make_config(false, Some("W4PHS"), None);
    let err = config.validate().unwrap_err();
    assert!(matches!(err, ConfigValidationError::OfflinePathHasCallsign));
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
    config.identity.callsign = Some("W4 PHS".into());
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
    let e = ConfigValidationError::CmsPathMissingCallsign;
    assert_eq!(e.to_string(), "CMS path requires identity.callsign to be set");

    let e = ConfigValidationError::OfflinePathHasCallsign;
    assert_eq!(e.to_string(),
        "offline path must NOT have identity.callsign set (use identity.identifier instead)");

    let e = ConfigValidationError::InvalidIdentity { field: "callsign", rule: "must not be empty" };
    assert_eq!(e.to_string(), "invalid identity field `callsign`: must not be empty");
}
