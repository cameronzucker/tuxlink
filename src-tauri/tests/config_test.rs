use tuxlink_lib::config::{Config, CONFIG_SCHEMA_VERSION};

#[test]
fn test_deserialize_minimal_valid_config() {
    let json = r#"{
        "schema_version": 1,
        "callsign": "W4PHS",
        "grid_square": "EM75xx",
        "pat_mbo_address": "W4PHS@winlink.org",
        "winlink_password_present": true,
        "wizard_completed": true
    }"#;
    let config: Config = serde_json::from_str(json).expect("must deserialize");
    assert_eq!(config.schema_version, CONFIG_SCHEMA_VERSION);
    assert_eq!(config.callsign, "W4PHS");
    assert_eq!(config.grid_square, "EM75xx");
    assert_eq!(config.pat_mbo_address, "W4PHS@winlink.org");
    assert!(config.winlink_password_present);
    assert!(config.wizard_completed);
}

#[test]
fn test_reject_wrong_schema_version() {
    let json = r#"{
        "schema_version": 99,
        "callsign": "W4PHS",
        "grid_square": "EM75xx",
        "pat_mbo_address": "W4PHS@winlink.org",
        "winlink_password_present": true,
        "wizard_completed": true
    }"#;
    let result: Result<Config, _> = serde_json::from_str(json);
    assert!(result.is_err(), "unexpected schema version must fail to deserialize");
}

#[test]
fn test_callsign_must_be_nonempty() {
    let json = r#"{
        "schema_version": 1,
        "callsign": "",
        "grid_square": "EM75xx",
        "pat_mbo_address": "W4PHS@winlink.org",
        "winlink_password_present": true,
        "wizard_completed": true
    }"#;
    let result: Result<Config, _> = serde_json::from_str(json);
    assert!(result.is_err(), "empty callsign must fail validation");
}

#[test]
fn test_config_path_uses_xdg_config_home_when_set() {
    std::env::set_var("XDG_CONFIG_HOME", "/tmp/tuxlink-test-xdg");
    let path = tuxlink_lib::config::config_path();
    assert_eq!(path, std::path::PathBuf::from("/tmp/tuxlink-test-xdg/tuxlink/config.json"));
    std::env::remove_var("XDG_CONFIG_HOME");
}
