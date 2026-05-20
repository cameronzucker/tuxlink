use std::path::{Path, PathBuf};
use std::time::Duration;
use tuxlink_lib::config::{
    CmsTransport, Config, ConnectConfig, GpsState, IdentityConfig, PositionPrecision,
    PrivacyConfig,
};
use tuxlink_lib::pat_process::{PatProcess, PatSpawnOptions};

/// Default announce timeout the app/bootstrap uses (was hardcoded in spawn
/// before tuxlink-22l §11.1). Tests pass a short value to exercise the
/// real-timeout path quickly.
const DEFAULT_ANNOUNCE_TIMEOUT: Duration = Duration::from_secs(10);

/// Write an executable `/bin/sh` fixture script to `dir`, returning its path.
/// The script stands in for `pat`: `PatProcess::spawn` invokes it with the
/// fixed argv `--config <p> --mbox <p> http --addr <listen>`, so the script
/// IGNORES its args except where noted in `body`. `body` is the shell body
/// run after the shebang.
fn write_fixture(dir: &Path, name: &str, body: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join(name);
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).expect("write fixture");
    let mut perms = std::fs::metadata(&path).expect("stat fixture").permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&path, perms).expect("chmod fixture");
    path
}

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
        http_announce_timeout: DEFAULT_ANNOUNCE_TIMEOUT,
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
        http_announce_timeout: DEFAULT_ANNOUNCE_TIMEOUT,
    };

    let pid_file = opts.pid_file.clone();
    let mut proc = PatProcess::spawn(opts).unwrap();
    assert!(pid_file.exists(), "pid file must exist while pat is running");
    proc.shutdown(Duration::from_secs(5)).unwrap();
    assert!(!pid_file.exists(), "pid file must be removed after graceful shutdown");
}

/// B1 (tuxlink-22l §11.1, adrev #7): the announce deadline must be REAL.
/// A Pat-like process that stays alive but never emits the listen-address
/// line must cause `spawn` to return `TimedOut` *at the deadline*, not hang
/// forever. The pre-22l code only re-checked the deadline BETWEEN lines, so
/// a child that blocks mid-line (alive, no newline) hangs the calling thread
/// indefinitely. A fixture that sleeps while writing nothing to stderr
/// reproduces that: `read_line` blocks with no bytes to read.
#[test]
fn spawn_times_out_when_no_announce_within_deadline() {
    let tmp = tempfile::tempdir().expect("tmpdir");
    // Stays alive 5s, never prints the needle (nothing on stderr at all).
    let fixture = write_fixture(tmp.path(), "silent-pat.sh", "sleep 5");

    let opts = PatSpawnOptions {
        binary: fixture,
        config_path: tmp.path().join("pat-config.json"),
        mbox_dir: tmp.path().join("mbox"),
        http_listen_port: 0,
        pid_file: tmp.path().join("pat.pid"),
        log_sink: None,
        tuxlink_config: minimal_cms_config(),
        http_announce_timeout: Duration::from_secs(1),
    };

    let start = std::time::Instant::now();
    let result = PatProcess::spawn(opts);
    let elapsed = start.elapsed();

    // Match rather than `expect_err` so we don't require `PatProcess: Debug`.
    let err = match result {
        Ok(_) => panic!("spawn must fail when Pat never announces"),
        Err(e) => e,
    };
    assert_eq!(
        err.kind(),
        std::io::ErrorKind::TimedOut,
        "announce-timeout must surface as TimedOut, got: {err:?}"
    );
    // The deadline is REAL: spawn returned near the 1s timeout, not after the
    // fixture's 5s sleep and not never. < 3s proves it did not block on the
    // child's lifetime / a never-arriving newline.
    assert!(
        elapsed < Duration::from_secs(3),
        "spawn must return at the deadline (~1s), not block; elapsed = {elapsed:?}"
    );
}

/// B5 (tuxlink-22l §11.1, adrev #1): NO startup stderr line is discarded.
/// Pre-22l, lines read before the announce were dropped on the calling
/// thread and only POST-announce lines reached `log_sink`. The unified
/// reader must forward EVERY line — including the announce line itself and
/// anything before it — so Pat's startup is visible downstream.
///
/// The fixture echoes the listen address it was handed (the LAST argv item,
/// i.e. `127.0.0.1:<port>` from `http --addr <listen>`) inside an
/// announce-style line, preceded by a pre-announce banner and followed by a
/// post-announce line, then sleeps so the process stays alive (mirrors Pat's
/// long-lived http server).
#[test]
fn spawn_forwards_all_stderr_lines_including_announce_to_log_sink() {
    let tmp = tempfile::tempdir().expect("tmpdir");
    // `last` ends up holding the `--addr` value (the final positional arg).
    // Emit: a pre-announce line, the announce line (contains the needle),
    // a post-announce line, then stay alive. All to stderr (fd 2).
    let fixture = write_fixture(
        tmp.path(),
        "chatty-pat.sh",
        r#"for a in "$@"; do last="$a"; done
echo "pre-announce banner" 1>&2
echo "Starting HTTP service ($last)" 1>&2
echo "post-announce line" 1>&2
sleep 5"#,
    );

    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let opts = PatSpawnOptions {
        binary: fixture,
        config_path: tmp.path().join("pat-config.json"),
        mbox_dir: tmp.path().join("mbox"),
        http_listen_port: 0,
        pid_file: tmp.path().join("pat.pid"),
        log_sink: Some(tx),
        tuxlink_config: minimal_cms_config(),
        http_announce_timeout: Duration::from_secs(5),
    };

    let mut proc = PatProcess::spawn(opts).expect("spawn must succeed once announce line is seen");

    // Collect lines the reader forwarded. The pre-announce banner proves the
    // line read BEFORE the announce was NOT discarded; the announce + post
    // lines prove forwarding continues through and past the announce. Bounded
    // poll so a regression (lost lines) fails instead of hanging.
    let mut got: Vec<String> = Vec::new();
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while std::time::Instant::now() < deadline {
        match rx.recv_timeout(Duration::from_millis(200)) {
            Ok(line) => {
                got.push(line);
                if got.len() >= 3 {
                    break;
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    proc.shutdown(Duration::from_secs(5)).expect("graceful shutdown");

    assert!(
        got.iter().any(|l| l.contains("pre-announce banner")),
        "pre-announce line must NOT be discarded; got: {got:?}"
    );
    assert!(
        got.iter().any(|l| l.contains("Starting HTTP service")),
        "announce line itself must be forwarded; got: {got:?}"
    );
    assert!(
        got.iter().any(|l| l.contains("post-announce line")),
        "post-announce line must be forwarded; got: {got:?}"
    );
    // Forwarded lines are newline-trimmed (per the log_sink contract).
    assert!(
        got.iter().all(|l| !l.ends_with('\n') && !l.ends_with('\r')),
        "forwarded lines must be newline-trimmed; got: {got:?}"
    );
}
