use std::path::PathBuf;
use std::time::Duration;
use tuxlink_lib::config::{
    CmsTransport, Config, ConnectConfig, GpsState, IdentityConfig, PositionPrecision,
    PrivacyConfig,
};
use tuxlink_lib::pat_process::{PatProcess, PatSpawnOptions};

/// This test requires a `pat` binary in PATH or at the path passed in.
/// CI installs Pat at a known location (see Task 19).
fn pat_binary() -> PathBuf {
    std::env::var_os("PAT_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("pat"))
}

/// Per tuxlink-756: PatSpawnOptions now carries `tuxlink_config` and
/// PatProcess::spawn renders Pat's config from it. Tests build a minimal
/// valid CMS-path config to satisfy the render contract; Pat's actual
/// CMS behavior is not exercised here (any keyring lookup would miss in
/// the test ENV but these tests only assert spawn + shutdown lifecycle).
fn minimal_cms_config() -> Config {
    Config {
        schema_version: 1,
        wizard_completed: true,
        connect: ConnectConfig {
            connect_to_cms: true,
            transport: CmsTransport::CmsSsl,
        },
        identity: IdentityConfig {
            callsign: Some("TEST1".to_string()),
            identifier: None,
            grid: Some("AA00aa".to_string()),
        },
        privacy: PrivacyConfig {
            gps_state: GpsState::Off,
            position_precision: PositionPrecision::FourCharGrid,
        },
        pat_mbo_address: None,
    }
}

#[test]
fn test_spawn_and_graceful_shutdown() {
    let tmp = tempfile::tempdir().expect("tmpdir");
    let opts = PatSpawnOptions {
        binary: pat_binary(),
        config_path: tmp.path().join("pat-config.json"),
        mbox_dir: tmp.path().join("mbox"),
        http_listen_port: 0,
        pid_file: tmp.path().join("pat.pid"),
        log_sink: None,
        tuxlink_config: minimal_cms_config(),
    };

    let mut proc = PatProcess::spawn(opts).expect("spawn");
    assert!(proc.is_running(), "pat must be running after spawn");
    let port = proc.http_port();
    assert!(port > 0, "http_port must be resolved after spawn");

    proc.shutdown(Duration::from_secs(5)).expect("graceful shutdown");
    assert!(!proc.is_running(), "pat must be stopped after shutdown");
}

#[test]
fn test_stale_pid_file_is_cleaned_after_shutdown() {
    let tmp = tempfile::tempdir().unwrap();
    let opts = PatSpawnOptions {
        binary: pat_binary(),
        config_path: tmp.path().join("pat-config.json"),
        mbox_dir: tmp.path().join("mbox"),
        http_listen_port: 0,
        pid_file: tmp.path().join("pat.pid"),
        log_sink: None,
        tuxlink_config: minimal_cms_config(),
    };

    let pid_file = opts.pid_file.clone();
    let mut proc = PatProcess::spawn(opts).unwrap();
    assert!(pid_file.exists(), "pid file must exist while pat is running");
    proc.shutdown(Duration::from_secs(5)).unwrap();
    assert!(!pid_file.exists(), "pid file must be removed after graceful shutdown");
}
