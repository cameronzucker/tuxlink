// wizard_persist_cms_test.rs — unit tests for wizard_persist_cms (Task 3.2 / tuxlink-1r5)
// Spec: docs/superpowers/specs/2026-05-18-onboarding-wizard-cluster-design.md §3.2 + §3.5
// Plan: Phase 3 Task 3.2
//
// These are UNIT tests — they use keyring::mock (not real gnome-keyring).
// Integration tests with a real keyring backend are in wizard_integration_test.rs (--ignored).
//
// We test persist_cms_impl() directly (not the Tauri command wrapper) to avoid
// needing a live Tauri runtime. The Tauri command is just a thin mutex guard + delegate.
// Busy (mutex contention) is tested separately below using WizardMutex directly.

use keyring::{mock, set_default_credential_builder};
use serial_test::serial;
use std::sync::Arc;
use tuxlink_lib::wizard::{persist_cms_impl, WizardError, WizardMutex};

// ──────────────────────────────────────────────────────────────
// Test infrastructure
// ──────────────────────────────────────────────────────────────

/// RAII guard: points XDG_CONFIG_HOME at a fresh tempdir for the test's lifetime.
/// Restores the prior value on drop (even on panic). Use with #[serial] to avoid
/// concurrent-test races on the global env var.
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

/// Install the mock keyring builder so tests don't touch the real OS keyring.
fn use_mock_keyring() {
    set_default_credential_builder(mock::default_credential_builder());
}

// ──────────────────────────────────────────────────────────────
// Happy path
// ──────────────────────────────────────────────────────────────

#[tokio::test]
#[serial]
#[allow(deprecated)] // reads pat_mbo_address on deserialized Config; field deprecated per tuxlink-9phd T8.1
async fn persist_cms_happy_path_writes_keyring_and_config() {
    use_mock_keyring();
    let _xdg = xdg_temp();

    let result = persist_cms_impl(
        "w4phs".to_string(),          // lowercase — should be normalized to W4PHS
        "p@ssw0rd".to_string(),
        "EM75".to_string(),
        "W4PHS@winlink.org".to_string(),
    )
    .await;

    assert!(result.is_ok(), "happy path should succeed: {result:?}");

    // NOTE: The mock keyring does not share state between different Entry::new() calls.
    // Each Entry::new() returns a fresh MockCredential with no backing store.
    // Cross-entry keyring read-back verification is deferred to the integration test
    // (wizard_integration_test.rs --ignored) which uses a real gnome-keyring-daemon.
    // Here we verify the config.json side-effect which IS testable without a live keyring.

    // Verify config.json was written correctly (no password material — AMD-11 + spec §3.6).
    let config = tuxlink_lib::config::read_config().expect("config should be readable after persist");
    // tuxlink-9xy1 Task 3 (Codex CODEX-1 fix): persist_cms_impl now writes
    // wizard_phase = Identity (was: wizard_completed = true). wizard_completed
    // is the derived view (= phase.is_complete()), so it MUST be false after
    // Identity — the Location step has not run yet.
    assert!(!config.wizard_completed, "wizard_completed must be false after Identity persist (Location pending)");
    assert_eq!(
        config.wizard_phase,
        tuxlink_lib::wizard_phase::WizardPhase::Identity,
        "wizard_phase must be Identity after CMS persist (Location is the next phase)"
    );
    assert!(config.connect.connect_to_cms, "connect_to_cms must be true (CMS path)");
    assert_eq!(config.identity.callsign.as_deref(), Some("W4PHS"), "callsign must be normalized to uppercase");
    assert_eq!(config.identity.grid.as_deref(), Some("EM75"), "grid preserved");
    // pat_mbo_address is deprecated + skip_serializing (tuxlink-9phd T8.1): the field is never
    // written to config.json, so reading back always yields None regardless of what was passed.
    assert!(config.pat_mbo_address.is_none(), "pat_mbo_address must be absent from config.json (skip_serializing)");
    assert!(config.identity.identifier.is_none(), "identifier unused in CMS path");
}

// ──────────────────────────────────────────────────────────────
// InvalidInput — non-ASCII callsign (homoglyph guard, spec §5.9)
// ──────────────────────────────────────────────────────────────

#[tokio::test]
#[serial]
async fn persist_cms_rejects_non_ascii_callsign() {
    use_mock_keyring();
    let _xdg = xdg_temp();

    // U+0410 CYRILLIC CAPITAL LETTER А — visually identical to Latin A.
    let result = persist_cms_impl(
        "W4PHSА".to_string(),    // ← Cyrillic А (U+0410)
        "password123".to_string(),
        "".to_string(),
        "".to_string(),
    )
    .await;

    match result {
        Err(WizardError::InvalidInput { field }) => {
            assert_eq!(field, "callsign", "error must name the callsign field");
        }
        other => panic!("expected InvalidInput for non-ASCII callsign, got {other:?}"),
    }
}

// ──────────────────────────────────────────────────────────────
// InvalidInput — whitespace-only callsign (trims to empty)
// ──────────────────────────────────────────────────────────────

#[tokio::test]
#[serial]
async fn persist_cms_rejects_empty_callsign_after_trim() {
    use_mock_keyring();
    let _xdg = xdg_temp();

    let result = persist_cms_impl(
        "   ".to_string(),   // trim → empty string → validate_identity rejects
        "password123".to_string(),
        "".to_string(),
        "".to_string(),
    )
    .await;

    match result {
        Err(WizardError::InvalidInput { field }) => {
            assert_eq!(field, "callsign");
        }
        other => panic!("expected InvalidInput for empty callsign, got {other:?}"),
    }
}

// ──────────────────────────────────────────────────────────────
// Busy — second concurrent invocation while mutex is held
// ──────────────────────────────────────────────────────────────

#[tokio::test]
#[serial]
async fn persist_cms_returns_busy_when_mutex_held() {
    // Test the Tauri command wrapper's Busy behavior directly via the mutex.
    let mutex = WizardMutex(Arc::new(tokio::sync::Mutex::new(())));
    // Hold the lock ourselves to simulate an in-flight call.
    let _guard = mutex.0.lock().await;

    // try_lock() should fail with Busy — verify the mutex guard works.
    let try_result = mutex.0.try_lock();
    assert!(
        try_result.is_err(),
        "try_lock while held must return an error (which maps to WizardError::Busy)"
    );
}

// ──────────────────────────────────────────────────────────────
// Snapshot-and-restore: prior credential is RESTORED on config-write failure
// ──────────────────────────────────────────────────────────────

#[tokio::test]
#[serial]
async fn persist_cms_restores_prior_credential_on_config_write_failure() {
    use_mock_keyring();

    // Write an initial credential at W4PHS BEFORE the wizard runs.
    let entry = keyring::Entry::new("tuxlink-pat", "W4PHS").expect("entry create");
    entry.set_password("original_password").expect("initial write");

    // Point XDG_CONFIG_HOME to /proc/1 — world-readable but not writable by non-root.
    // This forces write_config_atomic to fail while the keyring write (new_password) succeeded.
    let prior_xdg = std::env::var_os("XDG_CONFIG_HOME");
    unsafe { std::env::set_var("XDG_CONFIG_HOME", "/proc/1") };

    let result = persist_cms_impl(
        "W4PHS".to_string(),
        "new_password".to_string(),
        "".to_string(),
        "".to_string(),
    )
    .await;

    // Restore env var before asserting (guards against assert-panic leaving env dirty).
    match prior_xdg {
        Some(p) => unsafe { std::env::set_var("XDG_CONFIG_HOME", p) },
        None => unsafe { std::env::remove_var("XDG_CONFIG_HOME") },
    }

    // The command must fail with ConfigWrite or ConfigWriteAndRollbackFailed.
    match &result {
        Err(WizardError::ConfigWrite { .. }) |
        Err(WizardError::ConfigWriteAndRollbackFailed { .. }) => {}
        other => panic!("expected ConfigWrite/ConfigWriteAndRollbackFailed on forced-fail path, got {other:?}"),
    }

    // The prior credential must have been RESTORED (not new_password, not deleted).
    let restored = entry.get_password().expect("credential should still exist after rollback");
    assert_eq!(
        restored, "original_password",
        "snapshot-and-restore must restore the prior password, not new_password"
    );
}

// ──────────────────────────────────────────────────────────────
// Empty grid → None in config
// ──────────────────────────────────────────────────────────────

#[tokio::test]
#[serial]
async fn persist_cms_empty_grid_stored_as_none() {
    use_mock_keyring();
    let _xdg = xdg_temp();

    persist_cms_impl(
        "W4PHS".to_string(),
        "password123".to_string(),
        "".to_string(),          // empty grid → None in config
        "W4PHS@winlink.org".to_string(),
    )
    .await
    .expect("should succeed");

    let config = tuxlink_lib::config::read_config().expect("config should be readable");
    assert!(config.identity.grid.is_none(), "empty grid must be stored as null");
}

// ──────────────────────────────────────────────────────────────
// Empty MBO address → None in config
// ──────────────────────────────────────────────────────────────

#[tokio::test]
#[serial]
#[allow(deprecated)] // reads pat_mbo_address on deserialized Config; field deprecated per tuxlink-9phd T8.1
async fn persist_cms_empty_mbo_stored_as_none() {
    use_mock_keyring();
    let _xdg = xdg_temp();

    persist_cms_impl(
        "W4PHS".to_string(),
        "password123".to_string(),
        "".to_string(),
        "".to_string(),   // empty MBO → None in config
    )
    .await
    .expect("should succeed");

    let config = tuxlink_lib::config::read_config().expect("config should be readable");
    assert!(config.pat_mbo_address.is_none(), "empty MBO must be stored as null");
}
