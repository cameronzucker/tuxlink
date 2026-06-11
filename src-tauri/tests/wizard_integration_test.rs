// wizard_integration_test.rs — Phase 6.1 / tuxlink-1r5
// Spec: docs/superpowers/specs/2026-05-18-onboarding-wizard-cluster-design.md §3.8
// Plan: Phase 6 Task 6.1
//
// Integration tests that require a real keyring backend (gnome-keyring-daemon
// + dbus session bus). They write to the freedesktop Secret Service.
//
// ⚠ SAFETY (tuxlink-cnd): these MUST run in a THROWAWAY HOME so gnome-keyring
// uses a temp keyring dir — never the operator's real, cross-project-shared
// login keyring (the 2026-05-20 incident re-keyed it irrecoverably). Every test
// calls assert_keyring_isolated() first and FAILS CLOSED if HOME is not a
// sandbox. Do NOT run them with a bare `dbus-run-session -- cargo test
// --ignored` against your real HOME. Use the safe recipe:
//   docs/pitfalls/testing-pitfalls.md → "Headless real-keyring integration tests"
// In brief: HOME=$(mktemp -d), XDG_DATA_HOME under it, dbus-run-session, an
// in-sandbox gnome-keyring-daemon --unlock with an EMPTY password, then
//   cargo test --test wizard_integration_test --ignored
//
// All tests are #[ignore]d because they require a live secret-service D-Bus socket
// (DBUS_SESSION_BUS_ADDRESS must be set + gnome-keyring-daemon must be running).
// Normal `cargo test` skips them (but the SAFE unit test below always runs).
//
// Test cases per spec §3.8 (tuxlink-kc3q: de-Pat-framed; canonical service is "tuxlink"):
// 1. Direct keyring round-trip: Entry::new + set_password + get_password at the
//    (service="tuxlink", account=<callsign>) shape the app reads.
// 2. persist_cms_impl happy path: writes config.json + keyring; reads config back
//    AND asserts a SEPARATE process (`secret-tool`) reads the password back from
//    the freedesktop Secret Service — the real-store cross-process contract. A
//    secret-tool miss is a hard failure (it means the write went to the keyring
//    crate's in-process mock store, not the real Secret Service the app reads),
//    NOT a best-effort nicety.
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

/// RAII guard for a single env var: sets (or unsets) it, restores the prior
/// value on drop — even on panic. Used by the isolation-guard unit test so a
/// failed assertion can't leak a fake HOME into other #[serial] tests.
struct EnvGuard {
    key: &'static str,
    prior: Option<std::ffi::OsString>,
}

impl EnvGuard {
    fn set(key: &'static str, val: impl AsRef<std::ffi::OsStr>) -> Self {
        let prior = std::env::var_os(key);
        unsafe { std::env::set_var(key, val) };
        EnvGuard { key, prior }
    }
    fn unset(key: &'static str) -> Self {
        let prior = std::env::var_os(key);
        unsafe { std::env::remove_var(key) };
        EnvGuard { key, prior }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        match self.prior.take() {
            Some(v) => unsafe { std::env::set_var(self.key, v) },
            None => unsafe { std::env::remove_var(self.key) },
        }
    }
}

/// Resolve the directory the freedesktop Secret Service / gnome-keyring daemon
/// would use for on-disk keyring storage, from the CURRENT process environment,
/// per the XDG base-dir spec: `$XDG_DATA_HOME/keyrings`, else
/// `$HOME/.local/share/keyrings`. Returns None when neither is set (treated as
/// not-isolated by the guard below — fail closed).
fn resolve_keyring_dir() -> Option<std::path::PathBuf> {
    if let Some(data_home) = std::env::var_os("XDG_DATA_HOME") {
        if !data_home.is_empty() {
            return Some(std::path::Path::new(&data_home).join("keyrings"));
        }
    }
    let home = std::env::var_os("HOME")?;
    Some(std::path::Path::new(&home).join(".local/share/keyrings"))
}

/// True IFF the resolved keyring storage dir lives under the system temp dir —
/// i.e., a throwaway sandbox HOME/XDG_DATA_HOME (`mktemp -d` lands under
/// `std::env::temp_dir()`), NOT the operator's real `~/.local/share/keyrings`.
/// `temp_dir()` is derived from `$TMPDIR`/`/tmp`, independent of `$HOME`, so a
/// sandboxed env can't masquerade as real and vice-versa.
fn keyring_is_isolated() -> bool {
    match resolve_keyring_dir() {
        Some(dir) => dir.starts_with(std::env::temp_dir()),
        None => false,
    }
}

/// Fail the test CLOSED unless the keyring resolves to a throwaway sandbox dir.
/// MUST be called first in every real-keyring test, before any keyring op, so a
/// mis-invoked run aborts BEFORE it can write to the operator's real,
/// cross-project-shared login keyring (tuxlink-cnd; the 2026-05-20 incident).
fn assert_keyring_isolated() {
    assert!(
        keyring_is_isolated(),
        "REFUSING to run real-keyring test: keyring would resolve to {:?}, which is NOT under the \
         system temp dir ({:?}). These tests write to the freedesktop Secret Service and MUST run in \
         a throwaway HOME so gnome-keyring uses a temp keyring dir — otherwise they write to the \
         operator's REAL, cross-project-shared login keyring. Run via the safe recipe in \
         docs/pitfalls/testing-pitfalls.md (\"Headless real-keyring integration tests\"): \
         throwaway HOME + XDG_DATA_HOME under it + dbus-run-session + an in-sandbox \
         gnome-keyring-daemon.",
        resolve_keyring_dir(),
        std::env::temp_dir(),
    );
}

// ──────────────────────────────────────────────────────────────
// SAFE unit test (NOT #[ignore]d) — the tuxlink-cnd regression guard.
// Touches NO Secret Service: pure env + path logic, so it runs in normal
// `cargo test`/CI. If assert_keyring_isolated ever stops rejecting an
// un-isolated (real-HOME) environment, this fails here — long before any
// real keyring could be touched by the #[ignore]d integration tests below.
// ──────────────────────────────────────────────────────────────

#[test]
#[serial]
fn keyring_isolation_guard_detects_sandbox_vs_real_home() {
    // (1) A throwaway XDG_DATA_HOME under the system temp dir → ISOLATED.
    let tmp = tempfile::tempdir().expect("must create tempdir");
    {
        let _data = EnvGuard::set("XDG_DATA_HOME", tmp.path());
        assert!(
            keyring_is_isolated(),
            "a throwaway XDG_DATA_HOME under {:?} must read as isolated; resolved {:?}",
            std::env::temp_dir(),
            resolve_keyring_dir(),
        );
    }

    // (2) A realistic operator HOME (NOT under temp), XDG_DATA_HOME unset → the
    //     ~/.local/share/keyrings fallback path → NOT isolated (fail closed).
    {
        let _data = EnvGuard::unset("XDG_DATA_HOME");
        let _home = EnvGuard::set("HOME", "/home/operator-not-a-sandbox");
        assert!(
            !keyring_is_isolated(),
            "a real (non-temp) HOME must read as NOT isolated so the guard fails closed",
        );
        // The hard guard must PANIC on the un-isolated case (the abort that
        // prevents a real-keyring write).
        let panicked =
            std::panic::catch_unwind(assert_keyring_isolated).is_err();
        assert!(
            panicked,
            "assert_keyring_isolated must panic when the keyring is not isolated",
        );
    }
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

/// Verify that keyring::Entry::new("tuxlink", "W4PHS").set_password()
/// round-trips through the real gnome-keyring-daemon (or equivalent) and
/// can be read back, confirming the (service, account) shape the app reads.
///
/// This proves the wizard's credential lands in the real freedesktop Secret
/// Service over D-Bus (via the Rust `keyring` crate), not an in-process mock —
/// so the app's later credentials::read_password finds it.
#[tokio::test]
#[ignore]
#[serial]
async fn integration_keyring_round_trip_at_tuxlink_account_shape() {
    if skip_if_no_session_bus() { return; }
    assert_keyring_isolated();

    let callsign = "TUXTEST1";
    let password = "integration-test-password-do-not-use-in-production";

    // Write.
    let entry = keyring::Entry::new("tuxlink", callsign)
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
/// - Writes the password to keyring at (service="tuxlink", username="INTTEST2")
/// - Writes config.json to a temp dir
/// - Reads both back and asserts correctness
///
/// CONTRACT NOTE: the freedesktop Secret Service attribute that the Rust `keyring`
/// crate (via `dbus-secret-service`) writes for the entry's account is `username`
/// (NOT `account`). So the faithful cross-process read-back uses
/// `secret-tool lookup service tuxlink username INTTEST2`. Querying `account`
/// here would model nothing real and would falsely fail.
#[tokio::test]
#[ignore]
#[serial]
#[allow(deprecated)] // reads pat_mbo_address on deserialized Config; field deprecated per tuxlink-9phd T8.1
async fn integration_persist_cms_happy_path_real_keyring() {
    if skip_if_no_session_bus() { return; }
    assert_keyring_isolated();
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
    assert_eq!(config.identity.active_full.as_deref(), Some("INTTEST2"), "callsign normalized to uppercase");
    assert_eq!(config.identity.grid.as_deref(), Some("FM18"), "grid preserved");
    // pat_mbo_address is deprecated + skip_serializing (tuxlink-9phd T8.1): the field is never
    // written to config.json, so reading back always yields None regardless of what was passed.
    assert!(config.pat_mbo_address.is_none(), "pat_mbo_address must be absent from config.json (skip_serializing)");
    assert!(config.identity.identifier.is_none(), "CMS path must not set identifier");

    // CROSS-PROCESS CONTRACT ASSERTION (the load-bearing check, not best-effort).
    //
    // `secret-tool` is a SEPARATE process that reads the freedesktop Secret Service
    // over D-Bus — the same real store the app reads via the keyring crate. If the
    // wizard's keyring write landed in the crate's in-process mock store (the bug
    // when no Secret Service feature is enabled), this lookup MISSES and the app
    // would never find the credential. Asserting a successful, value-matching
    // read-back from a separate process IS the real-store contract. It must
    // succeed and match — a miss is a contract failure, not "an implementation
    // detail of the backend."
    //
    // We query by `{service, username}` — the exact attribute pair the Rust keyring
    // crate writes (verified: `attribute.service` + `attribute.username`). This is
    // the real on-disk query, not the freedesktop `account` convention.
    let output = std::process::Command::new("secret-tool")
        .args(["lookup", "service", "tuxlink", "username", "INTTEST2"])
        .output()
        .expect("secret-tool must be installed for the cross-process contract assertion");
    assert!(
        output.status.success(),
        "secret-tool (separate process) MUST find the credential at \
         (service=tuxlink, username=INTTEST2). A miss means the wizard wrote to the \
         keyring crate's in-process mock store instead of the freedesktop Secret \
         Service, so the app would never read it. status={:?} stderr={}",
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
    let _ = keyring::Entry::new("tuxlink", "INTTEST2")
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
    assert_keyring_isolated();

    let callsign = "INTTEST3";
    let original = "original-password";

    // Pre-write the original credential to the real keyring.
    let entry = keyring::Entry::new("tuxlink", callsign).expect("entry create");
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
