// wizard_integration_test.rs — Phase 6.1 / tuxlink-1r5
// Spec: docs/superpowers/specs/2026-05-18-onboarding-wizard-cluster-design.md §3.8
// Plan: Phase 6 Task 6.1
//
// CI-only integration tests that require a real keyring backend (gnome-keyring-daemon
// + dbus session bus). Run with:
//   dbus-run-session -- cargo test --test wizard_integration_test --ignored
//
// All tests are #[ignore]d because they require a live secret-service D-Bus socket
// (DBUS_SESSION_BUS_ADDRESS must be set + gnome-keyring-daemon must be running).
// Normal `cargo test` skips them; CI runs them explicitly.
//
// Test cases per spec §3.8:
// 1. Direct keyring round-trip: Entry::new + set_password + get_password at the
//    exact (service="tuxlink-pat", account=<callsign>) shape that Pat reads.
// 2. persist_cms_impl happy path: writes config.json + keyring; reads config back
//    AND asserts a SEPARATE process (`secret-tool`) reads the password back from
//    the freedesktop Secret Service — the wizard→Pat cross-process contract. A
//    secret-tool miss is a hard failure (it means the write went to the keyring
//    crate's mock store, which Pat's go-keyring cannot see), NOT a best-effort
//    nicety.
// 3. Snapshot-and-restore: pre-write a credential, simulate config-write failure,
//    assert prior credential is restored (not overwritten, not deleted).

use serial_test::serial;
use tuxlink_lib::wizard::{persist_cms_impl, WizardError};

// ──────────────────────────────────────────────────────────────
// Test infrastructure
// ──────────────────────────────────────────────────────────────

/// RAII guard: scopes XDG_CONFIG_HOME to a fresh tempdir.
struct XdgGuard {
    prior: Option<std::ffi::OsString>,
    _tmp: tempfile::TempDir,
}

impl Drop for XdgGuard {
    fn drop(&mut self) {
        match self.prior.take() {
            Some(p) => unsafe { std::env::set_var("XDG_CONFIG_HOME", p) },
            None => unsafe { std::env::remove_var("XDG_CONFIG_HOME") },
        }
    }
}

fn xdg_temp() -> XdgGuard {
    let tmp = tempfile::tempdir().expect("must create tempdir");
    let path = tmp.path().to_owned();
    let prior = std::env::var_os("XDG_CONFIG_HOME");
    unsafe { std::env::set_var("XDG_CONFIG_HOME", &path) };
    XdgGuard { prior, _tmp: tmp }
}

/// Skip the test if DBUS_SESSION_BUS_ADDRESS is not set (real keyring unavailable).
/// Returns true if the skip was triggered (caller should return immediately).
fn skip_if_no_session_bus() -> bool {
    if std::env::var("DBUS_SESSION_BUS_ADDRESS").is_err() {
        eprintln!(
            "SKIP: DBUS_SESSION_BUS_ADDRESS not set — real keyring tests require \
             gnome-keyring-daemon + dbus session. Run with: \
             dbus-run-session -- cargo test --test wizard_integration_test --ignored"
        );
        return true;
    }
    false
}

// ──────────────────────────────────────────────────────────────
// Case 1: Direct keyring round-trip via keyring::Entry
// ──────────────────────────────────────────────────────────────

/// Verify that keyring::Entry::new("tuxlink-pat", "W4PHS").set_password()
/// round-trips through the real gnome-keyring-daemon (or equivalent) and
/// can be read back, confirming the (service, account) shape that Pat reads.
///
/// This is the cross-language contract test: the Rust `keyring` crate writes via
/// the freedesktop Secret Service D-Bus protocol; Pat's `go-keyring` reads via
/// the same protocol. If this round-trip works, the wizard→Pat handoff works.
#[tokio::test]
#[ignore]
#[serial]
async fn integration_keyring_round_trip_at_tuxlink_pat_account_shape() {
    if skip_if_no_session_bus() { return; }

    let callsign = "TUXTEST1";
    let password = "integration-test-password-do-not-use-in-production";

    // Write.
    let entry = keyring::Entry::new("tuxlink-pat", callsign)
        .expect("should create entry");
    entry.set_password(password).expect("should write to real keyring");

    // Read back.
    let read_back = entry.get_password().expect("should read from real keyring");
    assert_eq!(read_back, password, "keyring round-trip must preserve the password exactly");

    // Cleanup (best-effort; don't fail the test if deletion also fails).
    let _ = entry.delete_credential();
}

// ──────────────────────────────────────────────────────────────
// Case 2: persist_cms_impl happy path against real keyring
// ──────────────────────────────────────────────────────────────

/// Full persist_cms_impl run against the real gnome-keyring-daemon:
/// - Writes the password to keyring at (service="tuxlink-pat", username="INTTEST2")
/// - Writes config.json to a temp dir
/// - Reads both back and asserts correctness
///
/// CONTRACT NOTE: the freedesktop Secret Service attribute that the Rust `keyring`
/// crate (via `dbus-secret-service`) writes for the entry's account is `username`
/// (NOT `account`). zalando `go-keyring` — the reader on Pat's side — searches by
/// exactly `{service, username}` (go-keyring keyring_unix.go: `search := {"username":
/// user, "service": service}`). So the faithful cross-process read-back uses
/// `secret-tool lookup service tuxlink-pat username INTTEST2`. Querying `account`
/// here would model nothing real and would falsely fail.
#[tokio::test]
#[ignore]
#[serial]
async fn integration_persist_cms_happy_path_real_keyring() {
    if skip_if_no_session_bus() { return; }
    let _xdg = xdg_temp();

    let callsign = "INTTEST2";
    let password = "integration-test-password-2";

    let result = persist_cms_impl(
        callsign.to_lowercase(),   // persist_cms_impl normalizes to uppercase
        password.to_string(),
        "FM18".to_string(),
        "INTTEST2@winlink.org".to_string(),
    )
    .await;

    assert!(result.is_ok(), "persist_cms_impl should succeed against real keyring: {result:?}");

    // Assert config.json written correctly (no password material — AMD-11 + spec §3.6).
    // The keyring write is exercised by persist_cms_impl; if it returned Ok(()),
    // the write succeeded. The direct round-trip is separately verified by Case 1.
    let config = tuxlink_lib::config::read_config().expect("config.json should exist after persist");
    assert!(config.wizard_completed, "wizard_completed must be true after persist");
    assert!(config.connect.connect_to_cms, "connect_to_cms must be true for CMS path");
    assert_eq!(config.identity.callsign.as_deref(), Some("INTTEST2"), "callsign normalized to uppercase");
    assert_eq!(config.identity.grid.as_deref(), Some("FM18"), "grid preserved");
    assert_eq!(config.pat_mbo_address.as_deref(), Some("INTTEST2@winlink.org"), "MBO address stored");
    assert!(config.identity.identifier.is_none(), "CMS path must not set identifier");

    // CROSS-PROCESS CONTRACT ASSERTION (the load-bearing check, not best-effort).
    //
    // `secret-tool` is a SEPARATE process that reads the freedesktop Secret Service
    // over D-Bus — the exact same protocol+store that Pat's `go-keyring` reads. If
    // the wizard's keyring write landed in the crate's in-process mock store (the
    // bug when no Secret Service feature is enabled), this lookup MISSES and Pat
    // would never find the credential. Asserting a successful, value-matching
    // read-back from a separate process IS the wizard→Pat contract. It must
    // succeed and match — a miss is a contract failure, not "an implementation
    // detail of the backend."
    //
    // We query by `{service, username}` — the EXACT attribute pair zalando
    // go-keyring searches on Unix (keyring_unix.go), and the exact pair the Rust
    // keyring crate writes (verified: `attribute.service` + `attribute.username`).
    // This is the real reader's query, not the freedesktop `account` convention.
    let output = std::process::Command::new("secret-tool")
        .args(["lookup", "service", "tuxlink-pat", "username", "INTTEST2"])
        .output()
        .expect("secret-tool must be installed for the cross-process contract assertion");
    assert!(
        output.status.success(),
        "secret-tool (separate process) MUST find the credential at \
         (service=tuxlink-pat, username=INTTEST2) — the exact attribute pair Pat's \
         go-keyring searches. A miss means the wizard wrote to the keyring crate's \
         mock store instead of the freedesktop Secret Service, so tuxlink-pat's \
         go-keyring would never read it. status={:?} stderr={}",
        output.status,
        String::from_utf8_lossy(&output.stderr),
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        stdout.trim(),
        password,
        "secret-tool cross-process read-back must match the submitted password exactly"
    );

    // Cleanup: remove the credential persist_cms_impl wrote (separate from Case 1's entry).
    let _ = keyring::Entry::new("tuxlink-pat", "INTTEST2")
        .and_then(|e| e.delete_credential());
}

// ──────────────────────────────────────────────────────────────
// Case 3: Snapshot-and-restore with real keyring
// ──────────────────────────────────────────────────────────────

/// Verify that when the config write fails AFTER the keyring write succeeded,
/// the prior keyring entry is RESTORED (not overwritten, not deleted).
/// Uses the real keyring so the compensating-transaction restore is exercised
/// end-to-end against the gnome-keyring-daemon's actual behavior.
#[tokio::test]
#[ignore]
#[serial]
async fn integration_snapshot_and_restore_on_config_write_failure() {
    if skip_if_no_session_bus() { return; }

    let callsign = "INTTEST3";
    let original = "original-password";

    // Pre-write the original credential to the real keyring.
    let entry = keyring::Entry::new("tuxlink-pat", callsign).expect("entry create");
    entry.set_password(original).expect("pre-write should succeed");

    // Point XDG_CONFIG_HOME to /proc/1 — not writable by non-root.
    let prior_xdg = std::env::var_os("XDG_CONFIG_HOME");
    unsafe { std::env::set_var("XDG_CONFIG_HOME", "/proc/1") };

    let result = persist_cms_impl(
        callsign.to_string(),
        "should-be-rolled-back".to_string(),
        "".to_string(),
        "".to_string(),
    )
    .await;

    // Restore env.
    match prior_xdg {
        Some(p) => unsafe { std::env::set_var("XDG_CONFIG_HOME", p) },
        None => unsafe { std::env::remove_var("XDG_CONFIG_HOME") },
    }

    // Should fail with ConfigWrite or ConfigWriteAndRollbackFailed.
    match &result {
        Err(WizardError::ConfigWrite { .. }) |
        Err(WizardError::ConfigWriteAndRollbackFailed { .. }) => {}
        other => panic!(
            "expected ConfigWrite/ConfigWriteAndRollbackFailed on forced-fail path, got {other:?}"
        ),
    }

    // The ORIGINAL credential must have been restored.
    let restored = entry.get_password().expect("credential must still exist after rollback");
    assert_eq!(
        restored, original,
        "snapshot-and-restore must have restored the original credential, not the new one"
    );

    // Cleanup.
    let _ = entry.delete_credential();
}
