use std::path::PathBuf;
use std::time::Duration;
use tuxlink_lib::pat_process::{PatProcess, PatSpawnOptions};

/// This test requires a `pat` binary in PATH or at the path passed in.
/// CI installs Pat at a known location (see Task 19).
fn pat_binary() -> PathBuf {
    std::env::var_os("PAT_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("pat"))
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
    };
    std::fs::write(&opts.config_path, r#"{
        "mycall": "TEST1",
        "secure_login_password": "x",
        "locator": "AA00aa"
    }"#).unwrap();

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
    };
    std::fs::write(&opts.config_path, r#"{
        "mycall": "TEST1",
        "secure_login_password": "x",
        "locator": "AA00aa"
    }"#).unwrap();

    let pid_file = opts.pid_file.clone();
    let mut proc = PatProcess::spawn(opts).unwrap();
    assert!(pid_file.exists(), "pid file must exist while pat is running");
    proc.shutdown(Duration::from_secs(5)).unwrap();
    assert!(!pid_file.exists(), "pid file must be removed after graceful shutdown");
}
