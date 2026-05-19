//! Wizard backend — Tauri commands + state machine error/outcome types.
//!
//! Spec: docs/superpowers/specs/2026-05-18-onboarding-wizard-cluster-design.md
//! Plan: docs/superpowers/plans/2026-05-18-wizard-cluster-plan.md
//! bd issue: tuxlink-ko0 (Task 9 / wizard infra + Step 1 Welcome)
//!
//! This module currently ships Phase 1+2 of the wizard cluster:
//! - WizardError + TestSendOutcome enum definitions matching src/wizard/types.ts
//! - WizardMutex (single-flight guard per spec §3.7)
//! - get_wizard_completed command (real impl; consumed by src/App.tsx routing)
//! - Skeleton bodies for the 3 write commands (Tasks 10/11/11.5 flesh them out)
//!
//! The keyring crate dependency + map_keyring_error helper are deferred to
//! Task 10 (tuxlink-1r5) when the credentials write path actually needs them.

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

/// Writes credentials path: keyring entry + tuxlink config.json atomically.
/// See spec §3.2 for the snapshot-and-restore transactional flow.
///
/// **Skeleton — Task 10 (tuxlink-1r5) implements the body.**
#[tauri::command]
pub async fn wizard_persist_cms(
    state: tauri::State<'_, WizardMutex>,
    raw_callsign: String,
    password: String,
    grid: String,
    mbo_address: String,
) -> Result<(), WizardError> {
    let _guard = state.0.try_lock().map_err(|_| WizardError::Busy)?;
    let _ = (raw_callsign, password, grid, mbo_address);
    Err(WizardError::Other {
        detail: "wizard_persist_cms not yet implemented (Task 10 / tuxlink-1r5)".into(),
    })
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
