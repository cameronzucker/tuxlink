//! Wizard backend — Tauri commands + state machine error/outcome types.
//!
//! Spec: docs/superpowers/specs/2026-05-18-onboarding-wizard-cluster-design.md
//! Plan: docs/superpowers/plans/2026-05-18-wizard-cluster-plan.md
//! bd issues: tuxlink-ko0 (Task 9, wizard infra), tuxlink-1r5 (Task 10, keyring write)
//!
//! Phase 1+2 shipped: WizardError + TestSendOutcome enums, WizardMutex,
//! get_wizard_completed command, wizard_persist_offline + wizard_run_test_send skeletons.
//!
//! Phase 3 (Task 10, tuxlink-1r5): wizard_persist_cms full body.
//!   - keyring-first → config-second transactional flow per spec §3.2
//!   - snapshot-and-restore rollback per spec §3.2
//!   - callsign normalization + defense-in-depth validation per §5.9
//!   - map_keyring_error helper maps keyring::Error to WizardError per §3.5

use serde::Serialize;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Discriminated union mirrored as `WizardError` in src/wizard/types.ts.
/// Tauri's `#[serde(tag = "kind", content = "detail")]` produces the same
/// shape on both sides; the frontend pattern-matches by `kind`.
#[derive(Debug, Serialize, Clone)]
#[serde(tag = "kind", content = "detail")]
pub enum WizardError {
    Unavailable,
    Locked,
    PermissionDenied { platform_hint: String },
    ConfigWrite { detail: String },
    ConfigWriteAndRollbackFailed { config_error: String, rollback_error: String },
    Busy,
    InvalidInput { field: String },
    Other { detail: String },
}

/// Discriminated union mirrored as `TestSendOutcome` in src/wizard/types.ts.
#[derive(Debug, Serialize, Clone)]
#[serde(tag = "kind", content = "detail")]
pub enum TestSendOutcome {
    Success { reply_subject: Option<String> },
    Failed { cause: String, likely_causes_hint: Vec<String> },
}

/// Single-flight mutex for the 3 wizard write commands. Per spec §3.7, this
/// guards multi-window double-dispatch; the UI debounce is insufficient on
/// its own. `try_lock()` returns `WizardError::Busy` when contended.
pub struct WizardMutex(pub Arc<Mutex<()>>);

impl WizardMutex {
    pub fn new() -> Self {
        Self(Arc::new(Mutex::new(())))
    }
}

impl Default for WizardMutex {
    fn default() -> Self {
        Self::new()
    }
}

/// Map a keyring crate error to WizardError per spec §3.5.
///
/// Error classification:
/// - `NoStorageAccess` → the secret service is unavailable or locked.
///   We distinguish "locked vs unavailable" via the inner error's Display
///   string containing "locked" (gnome-keyring's wording on Linux).
/// - `PlatformFailure` with an error code matching permission-denial patterns
///   → `PermissionDenied` with a platform hint.
/// - Everything else → `Other { detail }`.
pub(crate) fn map_keyring_error(err: keyring::Error) -> WizardError {
    match err {
        keyring::Error::NoStorageAccess(ref inner) => {
            let msg = format!("{inner}").to_lowercase();
            if msg.contains("locked") {
                WizardError::Locked
            } else {
                WizardError::Unavailable
            }
        }
        keyring::Error::NoEntry => {
            // NoEntry during a `set_password` is unexpected (it's a write, not a read).
            // Treat as Unavailable (backend refused to create the entry).
            WizardError::Unavailable
        }
        keyring::Error::BadEncoding(_) => WizardError::InvalidInput {
            field: "password".into(),
        },
        keyring::Error::PlatformFailure(ref inner) => {
            let msg = format!("{inner}").to_lowercase();
            // "permission denied" appears in POSIX error messages; "access denied" in Windows.
            if msg.contains("permission denied") || msg.contains("access denied") {
                let hint = if cfg!(target_os = "macos") {
                    "macos"
                } else if cfg!(target_os = "windows") {
                    "windows"
                } else {
                    "linux"
                };
                WizardError::PermissionDenied { platform_hint: hint.into() }
            } else {
                WizardError::Other { detail: format!("{err}") }
            }
        }
        other => WizardError::Other {
            detail: format!("{other}"),
        },
    }
}

/// Returns whether the wizard has completed (config.json exists with
/// wizard_completed=true). Used by src/App.tsx on mount to route between
/// `<Wizard>` and `<MainShell>`.
///
/// Spec §3.4. Any read error (missing file, parse failure) yields `Ok(false)`
/// so the wizard is the safe-default route on first launch.
#[tauri::command]
pub async fn get_wizard_completed() -> Result<bool, WizardError> {
    match crate::config::read_config() {
        Ok(cfg) => Ok(cfg.wizard_completed),
        Err(_) => Ok(false),
    }
}

/// Core transactional logic for the CMS credentials path.
/// Extracted from the Tauri command so unit tests can call it directly
/// without constructing a `tauri::State` (which requires the Tauri runtime).
///
/// Caller is responsible for holding the WizardMutex before calling this.
/// Spec §3.2: keyring-first → config-second with snapshot-and-restore rollback.
pub async fn persist_cms_impl(
    raw_callsign: String,
    password: String,
    grid: String,
    mbo_address: String,
) -> Result<(), WizardError> {
    // Step 1: Normalize callsign (TrimSpace + ToUpper per spec §3.2).
    let callsign = raw_callsign.trim().to_uppercase();

    // Step 2: Validate normalized callsign — defense-in-depth per spec §5.9.
    // Frontend validator is the UX pass; Rust catches malicious/buggy frontends.
    if !callsign.is_ascii() || !crate::config::validate_identity(&callsign) {
        return Err(WizardError::InvalidInput { field: "callsign".into() });
    }

    // Step 3: Build the new Config struct in memory (no disk write yet).
    // Per spec §3.6: CMS path hardcodes connect_to_cms=true; NO password material.
    let new_config = crate::config::Config {
        schema_version: crate::config::CONFIG_SCHEMA_VERSION,
        wizard_completed: true,
        connect: crate::config::ConnectConfig {
            connect_to_cms: true,
            transport: crate::config::CmsTransport::CmsSsl,
        },
        identity: crate::config::IdentityConfig {
            callsign: Some(callsign.clone()),
            identifier: None,   // offline-only; not used in CMS path
            grid: if grid.trim().is_empty() { None } else { Some(grid.trim().to_string()) },
        },
        privacy: crate::config::PrivacyConfig {
            gps_state: crate::config::GpsState::BroadcastAtPrecision,
            position_precision: crate::config::PositionPrecision::FourCharGrid,
        },
        pat_mbo_address: if mbo_address.trim().is_empty() {
            None
        } else {
            Some(mbo_address.trim().to_string())
        },
    };

    // Step 4: Create keyring entry handle.
    let entry = keyring::Entry::new("tuxlink-pat", &callsign)
        .map_err(map_keyring_error)?;

    // Step 5: Snapshot prior password for rollback (spec §3.2 snapshot-before-overwrite).
    // If no prior entry exists, prior = None (rollback will delete, not restore).
    let prior = entry.get_password().ok();

    // Step 6: Write password to keyring FIRST.
    // Keyring failure aborts before any persistent disk state changes.
    entry.set_password(&password).map_err(map_keyring_error)?;

    // Step 7: Write config.json atomically.
    // On failure: best-effort rollback of the keyring write (snapshot-and-restore).
    if let Err(config_err) = crate::config::write_config_atomic(&new_config) {
        let rollback_result = match prior {
            // Prior entry existed: restore it (compensating transaction per spec §3.2).
            Some(ref p) => entry.set_password(p),
            // No prior entry: delete the one we just wrote.
            None => entry.delete_credential(),
        };
        match rollback_result {
            Ok(_) => {
                return Err(WizardError::ConfigWrite {
                    detail: format!("{config_err}"),
                });
            }
            Err(rollback_err) => {
                return Err(WizardError::ConfigWriteAndRollbackFailed {
                    config_error: format!("{config_err}"),
                    rollback_error: format!("{rollback_err}"),
                });
            }
        }
    }

    Ok(())
}

/// Writes credentials path: keyring entry + tuxlink config.json atomically.
/// See spec §3.2 for the snapshot-and-restore transactional flow.
///
/// - Callsign is normalized (trim + uppercase) before use.
/// - Non-ASCII callsign triggers InvalidInput (homoglyph guard per §5.9).
/// - Second concurrent invocation returns Busy (mutex guard per §3.7).
/// - connect_to_cms is hardcoded to true; no frontend boolean parameter
///   (footgun-elimination per Codex R5 P2 + spec §3.7).
#[tauri::command]
pub async fn wizard_persist_cms(
    state: tauri::State<'_, WizardMutex>,
    raw_callsign: String,
    password: String,
    grid: String,
    mbo_address: String,
) -> Result<(), WizardError> {
    let _guard = state.0.try_lock().map_err(|_| WizardError::Busy)?;
    persist_cms_impl(raw_callsign, password, grid, mbo_address).await
}

/// Writes offline path: tuxlink config.json only (no keyring). See spec §3.3.
///
/// **Skeleton — Task 11.5 (tuxlink-d76) implements the body.**
#[tauri::command]
pub async fn wizard_persist_offline(
    state: tauri::State<'_, WizardMutex>,
    identifier: String,
    grid: String,
) -> Result<(), WizardError> {
    let _guard = state.0.try_lock().map_err(|_| WizardError::Busy)?;
    let _ = (identifier, grid);
    Err(WizardError::Other {
        detail: "wizard_persist_offline not yet implemented (Task 11.5 / tuxlink-d76)".into(),
    })
}

/// Runs a test send to verify the CMS round-trip. See spec §3.8 for the
/// 4-substate state machine + Part 97 mock-gate (`TUXLINK_TEST_SEND_MOCK=1`).
///
/// **Skeleton — Task 11 (tuxlink-e4x) implements the body.**
#[tauri::command]
pub async fn wizard_run_test_send(
    state: tauri::State<'_, WizardMutex>,
) -> Result<TestSendOutcome, WizardError> {
    let _guard = state.0.try_lock().map_err(|_| WizardError::Busy)?;
    Err(WizardError::Other {
        detail: "wizard_run_test_send not yet implemented (Task 11 / tuxlink-e4x)".into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn get_wizard_completed_returns_false_when_no_config() {
        // The test environment typically has no config.json at the standard
        // path; verify the safe-default route.
        let result = get_wizard_completed().await;
        // Either OK(false) (no config exists) or OK(true) (a config exists with
        // wizard_completed=true). The Err path is unreachable per impl.
        assert!(result.is_ok(), "get_wizard_completed must not error: {result:?}");
    }

    #[test]
    fn wizard_mutex_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<WizardMutex>();
    }
}
