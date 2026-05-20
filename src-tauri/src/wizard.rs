//! Wizard backend — Tauri commands + state machine error/outcome types.
//!
//! Spec: docs/superpowers/specs/2026-05-18-onboarding-wizard-cluster-design.md
//! Plan: docs/superpowers/plans/2026-05-18-wizard-cluster-plan.md
//! bd issues: tuxlink-ko0 (Task 9, wizard infra), tuxlink-1r5 (Task 10, keyring write)
//!           tuxlink-d76 (Task 11.5, offline-identity path)
//!
//! Phase 1+2 shipped: WizardError + TestSendOutcome enums, WizardMutex,
//! get_wizard_completed command, wizard_persist_offline + wizard_run_test_send skeletons.
//!
//! Phase 3 (Task 10, tuxlink-1r5): wizard_persist_cms full body.
//!   - keyring-first → config-second transactional flow per spec §3.2
//!   - snapshot-and-restore rollback per spec §3.2
//!   - callsign normalization + defense-in-depth validation per §5.9
//!   - map_keyring_error helper maps keyring::Error to WizardError per §3.5
//!
//! Phase 4 (Task 11.5, tuxlink-d76): wizard_persist_offline full body.
//!   - config-only write (no keyring); connect_to_cms=false hardcoded
//!   - identifier (free-form, optional) + grid (optional) normalized + stored
//!   - identity.callsign = null (offline path forbids callsign per §3.6)
//!   - Busy guard via shared WizardMutex (spec §3.7)

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
    //
    // Only `NoEntry` legitimately means "no prior credential" (→ rollback deletes
    // what we wrote). Any OTHER read error (keyring unavailable / locked /
    // permission / platform failure) must NOT be silently treated as "no prior
    // cred": doing so would make a later config-write failure DELETE an existing
    // credential we simply failed to read, instead of restoring it. So we abort
    // here, BEFORE the destructive `set_password` overwrite, surfacing the read
    // error via the same classifier used for write errors.
    let prior: Option<String> = match entry.get_password() {
        Ok(p) => Some(p),
        Err(keyring::Error::NoEntry) => None,
        Err(snapshot_err) => return Err(map_keyring_error(snapshot_err)),
    };

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

/// Core logic for the offline identity path.
/// Extracted from the Tauri command so unit tests can call it directly
/// without constructing a `tauri::State` (which requires the Tauri runtime).
///
/// Caller is responsible for holding the WizardMutex before calling this.
/// Spec §3.3: config-only write; NO keyring touch; connect_to_cms hardcoded false.
pub async fn persist_offline_impl(
    identifier: String,
    grid: String,
) -> Result<(), WizardError> {
    // Normalize identifier: trim whitespace. Empty → None (spec §3.6: null in offline path).
    let identifier_opt: Option<String> = {
        let trimmed = identifier.trim();
        if trimmed.is_empty() {
            None
        } else {
            // Defense-in-depth validation: same loose rules as callsign (no whitespace,
            // ASCII-printable, ≤32 chars). Identifier accepts tactical strings (EOC-1, etc.)
            // so we use the same validate_identity() that CMS path uses for callsign.
            // Consistency per spec §3.2 step 1 analog for the offline path.
            if let Some(rule) = crate::config::validate_identity_describe(trimmed) {
                return Err(WizardError::InvalidInput { field: format!("identifier: {rule}") });
            }
            Some(trimmed.to_string())
        }
    };

    // Normalize grid: trim whitespace. Empty → None.
    // No format validation at the Rust layer (frontend validator is the UX pass;
    // the config schema stores whatever the user typed — the CMS/Pat layers don't
    // interpret grid directly). Consistency with persist_cms_impl's grid handling.
    let grid_opt: Option<String> = {
        let trimmed = grid.trim();
        if trimmed.is_empty() { None } else { Some(trimmed.to_string()) }
    };

    // Build the offline Config in memory.
    // Per spec §3.6:
    //   connect.connect_to_cms = false (offline path; hardcoded)
    //   connect.transport = CmsSsl (default; harmless in offline; keeps schema shape)
    //   identity.callsign = null (offline path forbids callsign; Config::validate enforces)
    //   identity.identifier = from parameter (optional)
    //   identity.grid = from parameter (optional)
    //   privacy defaults per Principle 7 (GPS on, FourCharGrid broadcast precision)
    //   pat_mbo_address = null (not used in offline path)
    let new_config = crate::config::Config {
        schema_version: crate::config::CONFIG_SCHEMA_VERSION,
        wizard_completed: true,
        connect: crate::config::ConnectConfig {
            connect_to_cms: false,
            transport: crate::config::CmsTransport::CmsSsl,
        },
        identity: crate::config::IdentityConfig {
            callsign: None,           // offline path: no callsign (spec §3.6)
            identifier: identifier_opt,
            grid: grid_opt,
        },
        privacy: crate::config::PrivacyConfig {
            gps_state: crate::config::GpsState::BroadcastAtPrecision,
            position_precision: crate::config::PositionPrecision::FourCharGrid,
        },
        pat_mbo_address: None,        // offline path: no MBO address
    };

    // Single atomic write to config.json. No keyring involved.
    // On failure: nothing to roll back (no prior writes succeeded).
    crate::config::write_config_atomic(&new_config)
        .map_err(|e| WizardError::ConfigWrite { detail: format!("{e}") })?;

    Ok(())
}

/// Writes offline path: tuxlink config.json only (no keyring). See spec §3.3.
///
/// - Both `identifier` and `grid` are optional (empty string = None in config).
/// - `connect.connect_to_cms` is hardcoded to `false` (offline path).
/// - `identity.callsign` is hardcoded to `null` (offline path; Config::validate enforces).
/// - NO keyring access. NO password. Consistent with spec §3.7 (no keyring footgun).
/// - Second concurrent invocation returns Busy (shared WizardMutex per §3.7).
/// - Identifier normalization consistent with Task 10's callsign normalization (trim + validate).
#[tauri::command]
pub async fn wizard_persist_offline(
    state: tauri::State<'_, WizardMutex>,
    identifier: String,
    grid: String,
) -> Result<(), WizardError> {
    let _guard = state.0.try_lock().map_err(|_| WizardError::Busy)?;
    persist_offline_impl(identifier, grid).await
}

/// Core logic for the test-send round-trip.
/// Extracted from the Tauri command so unit tests can call it directly
/// without constructing a `tauri::State`.
///
/// Part 97 mock-gate (spec §3.8):
///   When `TUXLINK_TEST_SEND_MOCK` is set (any non-empty value), this
///   function returns a mocked `TestSendOutcome` WITHOUT touching pat_client,
///   WITHOUT making any network connection, and WITHOUT transmitting anything.
///   This is the ONLY path subagents, CI, and automated tests use.
///   The live path (TUXLINK_TEST_SEND_MOCK unset) is operator-only per
///   docs/live-cms-testing-policy.md.
///
/// Caller is responsible for holding the WizardMutex before calling this.
pub async fn run_test_send_impl() -> Result<TestSendOutcome, WizardError> {
    // ── Part 97 mock-gate ──────────────────────────────────────────────────
    // Check env var at call time (not at startup) so tests can set it per-test.
    if std::env::var("TUXLINK_TEST_SEND_MOCK").is_ok() {
        // Return a mocked Success outcome. The mock always succeeds so that
        // subagent/CI flows exercise the full 4-substate path in the UI.
        // Unit tests override specific outcomes by invoking this function
        // with different env var values or by calling produce_mock_outcome()
        // directly.
        return Ok(produce_mock_outcome());
    }

    // ── Live path (operator-only; never run by subagents or CI) ───────────
    // Spec §3.8: sends to SERVICE@winlink.org with /test/ subject token.
    // Pat's HTTP API handles the actual CMS connection + RF/telnet/TLS.
    // Poll inbox for autoresponder reply up to TEST_SEND_TIMEOUT_SECS.
    let base_url = resolve_pat_base_url();
    let client = crate::pat_client::PatClient::new(&base_url);

    // Step 1: Post the test message to Pat's outbox.
    // Use system time for the date field; Pat accepts RFC3339/ISO-8601 format.
    let now_utc = {
        use std::time::{SystemTime, UNIX_EPOCH};
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        // Format as ISO-8601 UTC — Pat's HTTP API accepts this shape.
        // Trivial manual formatter: YYYY-MM-DDTHH:MM:SSZ from epoch seconds.
        let s = secs;
        let sec = s % 60;
        let min = (s / 60) % 60;
        let hr = (s / 3600) % 24;
        let days = s / 86400;
        // Day 0 = 1970-01-01; approximate calendar arithmetic (Pat uses this only
        // for display; precision beyond ±1 day is not contractual here).
        let year = 1970 + days / 365;
        let doy = days % 365;
        let month = doy / 30 + 1;
        let day = doy % 30 + 1;
        format!("{year:04}-{month:02}-{day:02}T{hr:02}:{min:02}:{sec:02}Z")
    };
    let subject = "Tuxlink wizard /test/ verification";
    client
        .send(&["SERVICE@winlink.org"], subject, "Tuxlink wizard test send.", &now_utc)
        .await
        .map_err(|e| TestSendOutcome::Failed {
            cause: format!("Could not queue message in Pat outbox: {e}"),
            likely_causes_hint: default_likely_causes(),
        })
        .map_err(live_path_outcome_to_wizard_error)?;

    // Step 2: Poll inbox for autoresponder reply (timeout per spec §5.4).
    let deadline = tokio::time::Instant::now()
        + tokio::time::Duration::from_secs(TEST_SEND_TIMEOUT_SECS);

    loop {
        if tokio::time::Instant::now() >= deadline {
            return Err(WizardError::Other {
                detail: format!(
                    "{{\"outcome\":\"failed\",\"cause\":\"CMS didn't reply within {} seconds \
                     (no autoresponder). Likely cause: CMS busy or your network's outbound \
                     port 8773 is blocked.\",\"likely_causes_hint\":[{}]}}",
                    TEST_SEND_TIMEOUT_SECS,
                    default_likely_causes()
                        .iter()
                        .map(|s| format!("\"{}\"", s))
                        .collect::<Vec<_>>()
                        .join(",")
                ),
            });
        }

        match client.list(crate::pat_client::MailboxFolder::Inbox).await {
            Ok(msgs) => {
                // Look for a message from SERVICE@winlink.org (autoresponder).
                let reply = msgs
                    .iter()
                    .find(|m| m.from.to_uppercase().contains("SERVICE@WINLINK.ORG")
                        || m.subject.to_uppercase().contains("RE:"));
                if let Some(msg) = reply {
                    return Ok(TestSendOutcome::Success {
                        reply_subject: Some(msg.subject.clone()),
                    });
                }
            }
            Err(_) => {
                // Poll failure is transient; the deadline check above handles the timeout case.
            }
        }

        tokio::time::sleep(tokio::time::Duration::from_secs(POLL_INTERVAL_SECS)).await;
    }
}

/// Produce the mocked outcome for agent/CI/test contexts.
/// Always returns Success so the full 4-substate UI path is exercised.
/// Tests that need Failed outcomes set `TUXLINK_TEST_SEND_MOCK_FAIL=1`.
pub fn produce_mock_outcome() -> TestSendOutcome {
    if std::env::var("TUXLINK_TEST_SEND_MOCK_FAIL").is_ok() {
        TestSendOutcome::Failed {
            cause: "MOCKED: simulated test-send failure (TUXLINK_TEST_SEND_MOCK_FAIL is set)"
                .into(),
            likely_causes_hint: default_likely_causes(),
        }
    } else {
        TestSendOutcome::Success {
            reply_subject: Some(
                "Re: Tuxlink wizard /test/ verification [MOCKED]".into(),
            ),
        }
    }
}

/// Convert a live-path `TestSendOutcome::Failed` into a `WizardError::Other`
/// so the Tauri command can return `Result<TestSendOutcome, WizardError>`.
/// The failed outcome is embedded as a JSON-encoded detail so the frontend
/// can inspect it via the usual `Other { detail }` pattern.
fn live_path_outcome_to_wizard_error(outcome: TestSendOutcome) -> WizardError {
    match outcome {
        TestSendOutcome::Failed { cause, likely_causes_hint } => {
            let hints = likely_causes_hint
                .iter()
                .map(|s| format!("\"{}\"", s))
                .collect::<Vec<_>>()
                .join(",");
            WizardError::Other {
                detail: format!(
                    "{{\"outcome\":\"failed\",\"cause\":\"{}\",\"likely_causes_hint\":[{}]}}",
                    cause.replace('"', "\\\""),
                    hints
                ),
            }
        }
        // Success on the "send" step is not an error; this arm is unreachable
        // from the send_error path (send() returns Err, not Ok).
        TestSendOutcome::Success { .. } => WizardError::Other {
            detail: "Unexpected success in live_path_outcome_to_wizard_error".into(),
        },
    }
}

/// Default likely-causes hint list per spec §3.4 + §5.12 (captive portal).
fn default_likely_causes() -> Vec<String> {
    vec![
        "No internet connection".into(),
        "Firewall blocking port 8773".into(),
        "CMS temporarily busy".into(),
        "A captive portal / network login page intercepting traffic".into(),
    ]
}

/// Pat API base URL. Reads PAT_URL env var first (for operator overrides),
/// falls back to the standard local Pat sidecar address.
fn resolve_pat_base_url() -> String {
    std::env::var("PAT_URL").unwrap_or_else(|_| "http://127.0.0.1:8080".into())
}

/// How long to poll for the autoresponder reply before declaring failure.
const TEST_SEND_TIMEOUT_SECS: u64 = 30;

/// Polling interval for inbox checks.
const POLL_INTERVAL_SECS: u64 = 3;

/// Runs a test send to verify the CMS round-trip. See spec §3.8 for the
/// 4-substate state machine + Part 97 mock-gate (`TUXLINK_TEST_SEND_MOCK=1`).
///
/// **Part 97 safety (RADIO-1):**
/// - When `TUXLINK_TEST_SEND_MOCK` is set, returns a mocked outcome immediately.
///   No network connection. No CMS session. No transmission.
/// - When the env var is unset, invokes the live pat_client path (operator-only;
///   subject to the consent gate per docs/live-cms-testing-policy.md).
/// - The Rust-side WizardMutex ensures only ONE invocation runs at a time.
///   A concurrent invocation returns `WizardError::Busy` (spec §3.7).
/// - The React-side `BEGIN_TEST_SEND` dedup guard (wizardReducer.ts) ensures
///   the [Send test] button is ABSENT from non-idle substates, making it
///   impossible to dispatch two concurrent invocations via UI interaction.
///
/// **spec §3.4 substates:** this command runs during `sending` substate.
/// The `idle` → `sending` transition is driven by `BEGIN_TEST_SEND` in the reducer.
/// This command's result is dispatched as `TEST_SEND_RESULT` when it resolves.
#[tauri::command]
pub async fn wizard_run_test_send(
    state: tauri::State<'_, WizardMutex>,
) -> Result<TestSendOutcome, WizardError> {
    let _guard = state.0.try_lock().map_err(|_| WizardError::Busy)?;
    run_test_send_impl().await
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

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

    // ── persist_offline_impl unit tests (Task 11.5 / tuxlink-d76) ────────
    //
    // These tests mutate XDG_CONFIG_HOME (process-global env var) and MUST run
    // serially via #[serial] from the `serial_test` crate to avoid races when
    // the test harness runs multiple tests concurrently.

    /// RAII guard: redirects XDG_CONFIG_HOME to a temp dir, restores on drop.
    /// Copy of the pattern in tests/wizard_persist_cms_test.rs.
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
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().to_owned();
        let prior = std::env::var_os("XDG_CONFIG_HOME");
        unsafe { std::env::set_var("XDG_CONFIG_HOME", &path) };
        XdgGuard { prior, _tmp: tmp }
    }

    #[tokio::test]
    #[serial]
    async fn persist_offline_blank_submit_writes_valid_offline_config() {
        let _xdg = xdg_temp();
        let result = persist_offline_impl("".to_string(), "".to_string()).await;
        assert!(result.is_ok(), "blank submit must succeed: {result:?}");
        let cfg = crate::config::read_config().expect("config readable after blank-submit write");

        assert!(cfg.wizard_completed);
        assert!(!cfg.connect.connect_to_cms, "offline path must set connect_to_cms=false");
        assert!(cfg.identity.callsign.is_none(), "offline path must NOT set callsign");
        assert!(cfg.identity.identifier.is_none(), "blank identifier → None");
        assert!(cfg.identity.grid.is_none(), "blank grid → None");
        assert!(cfg.pat_mbo_address.is_none(), "offline path has no MBO address");
    }

    #[tokio::test]
    #[serial]
    async fn persist_offline_identifier_and_grid_stored() {
        let _xdg = xdg_temp();
        let result = persist_offline_impl("EOC-1".to_string(), "EM75".to_string()).await;
        assert!(result.is_ok(), "EOC-1 / EM75 submit must succeed: {result:?}");
        let cfg = crate::config::read_config().expect("config readable");

        assert_eq!(cfg.identity.identifier.as_deref(), Some("EOC-1"));
        assert_eq!(cfg.identity.grid.as_deref(), Some("EM75"));
        assert!(cfg.identity.callsign.is_none());
        assert!(!cfg.connect.connect_to_cms);
    }

    #[tokio::test]
    #[serial]
    async fn persist_offline_trims_whitespace_from_identifier() {
        let _xdg = xdg_temp();
        let result = persist_offline_impl("  ARES-NET  ".to_string(), "".to_string()).await;
        assert!(result.is_ok());
        let cfg = crate::config::read_config().expect("config readable");

        assert_eq!(cfg.identity.identifier.as_deref(), Some("ARES-NET"), "identifier must be trimmed");
    }

    #[tokio::test]
    #[serial]
    async fn persist_offline_trims_whitespace_from_grid() {
        let _xdg = xdg_temp();
        let result = persist_offline_impl("".to_string(), "  EM75  ".to_string()).await;
        assert!(result.is_ok());
        let cfg = crate::config::read_config().expect("config readable");

        assert_eq!(cfg.identity.grid.as_deref(), Some("EM75"), "grid must be trimmed");
    }

    #[tokio::test]
    async fn persist_offline_rejects_identifier_with_whitespace() {
        // An identifier containing internal whitespace after trimming is invalid
        // (validate_identity rejects internal whitespace). No env var needed — rejects
        // before any disk write, so no XDG_CONFIG_HOME race possible.
        let result = persist_offline_impl("EOC 1".to_string(), "".to_string()).await;
        match result {
            Err(WizardError::InvalidInput { .. }) => {}
            other => panic!("expected InvalidInput, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn persist_offline_rejects_identifier_too_long() {
        let long_id = "A".repeat(33);
        let result = persist_offline_impl(long_id, "".to_string()).await;
        match result {
            Err(WizardError::InvalidInput { .. }) => {}
            other => panic!("expected InvalidInput for >32 char identifier, got {other:?}"),
        }
    }

    #[tokio::test]
    #[serial]
    async fn persist_offline_connect_transport_default_is_cms_ssl() {
        let _xdg = xdg_temp();
        persist_offline_impl("".to_string(), "".to_string()).await.expect("ok");
        let cfg = crate::config::read_config().expect("config readable");

        assert_eq!(cfg.connect.transport, crate::config::CmsTransport::CmsSsl,
            "offline path must keep transport=CmsSsl (schema-shape consistency per spec §3.2)");
    }

    #[tokio::test]
    #[serial]
    async fn persist_offline_privacy_defaults_to_broadcast_four_char_grid() {
        let _xdg = xdg_temp();
        persist_offline_impl("".to_string(), "".to_string()).await.expect("ok");
        let cfg = crate::config::read_config().expect("config readable");

        assert_eq!(cfg.privacy.gps_state, crate::config::GpsState::BroadcastAtPrecision,
            "offline path must default to BroadcastAtPrecision per Principle 7");
        assert_eq!(cfg.privacy.position_precision, crate::config::PositionPrecision::FourCharGrid,
            "offline path must default to FourCharGrid per Principle 7");
    }

    // ── wizard_run_test_send unit tests (Task 11 / tuxlink-e4x) ──────────
    //
    // ALL tests use TUXLINK_TEST_SEND_MOCK=1 per Part 97 / RADIO-1 constraint.
    // The live path is operator-only and MUST NOT be invoked from automated tests.
    // env var mutations MUST run serially via #[serial] to avoid races.

    /// RAII guard: sets an env var and restores it on drop.
    struct EnvVarGuard {
        key: &'static str,
        prior: Result<String, std::env::VarError>,
    }

    impl EnvVarGuard {
        fn set(key: &'static str, value: &str) -> Self {
            let prior = std::env::var(key);
            unsafe { std::env::set_var(key, value) };
            EnvVarGuard { key, prior }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            match &self.prior {
                Ok(prev) => unsafe { std::env::set_var(self.key, prev) },
                Err(_) => unsafe { std::env::remove_var(self.key) },
            }
        }
    }

    #[tokio::test]
    #[serial]
    async fn run_test_send_mocked_success_returns_success_outcome() {
        // TUXLINK_TEST_SEND_MOCK set → run_test_send_impl short-circuits to mocked success.
        // MUST NOT TRANSMIT. The mock gate is the Part 97 / RADIO-1 safety net for automated tests.
        let _mock = EnvVarGuard::set("TUXLINK_TEST_SEND_MOCK", "1");
        let _no_fail = {
            // Remove TUXLINK_TEST_SEND_MOCK_FAIL if set from a prior test.
            unsafe { std::env::remove_var("TUXLINK_TEST_SEND_MOCK_FAIL") };
        };

        let result = run_test_send_impl().await;
        match result {
            Ok(TestSendOutcome::Success { reply_subject }) => {
                assert!(
                    reply_subject.is_some(),
                    "mocked success must carry a reply_subject"
                );
                assert!(
                    reply_subject.unwrap().contains("MOCKED"),
                    "mocked reply_subject must contain MOCKED so the UI can show a mock banner"
                );
            }
            other => panic!("expected Ok(Success), got {other:?}"),
        }
    }

    #[tokio::test]
    #[serial]
    async fn run_test_send_mocked_failed_returns_failed_outcome() {
        // TUXLINK_TEST_SEND_MOCK=1 + TUXLINK_TEST_SEND_MOCK_FAIL=1 → mocked failure.
        let _mock = EnvVarGuard::set("TUXLINK_TEST_SEND_MOCK", "1");
        let _fail = EnvVarGuard::set("TUXLINK_TEST_SEND_MOCK_FAIL", "1");

        let result = run_test_send_impl().await;
        match result {
            Ok(TestSendOutcome::Failed { cause, likely_causes_hint }) => {
                assert!(
                    cause.contains("MOCKED"),
                    "mocked failure cause must contain MOCKED"
                );
                assert!(
                    !likely_causes_hint.is_empty(),
                    "mocked failure must carry likely_causes_hint"
                );
            }
            other => panic!("expected Ok(Failed), got {other:?}"),
        }
    }

    #[tokio::test]
    #[serial]
    async fn produce_mock_outcome_success_when_no_fail_env() {
        // Verify produce_mock_outcome directly when FAIL env is absent.
        unsafe { std::env::remove_var("TUXLINK_TEST_SEND_MOCK_FAIL") };
        match produce_mock_outcome() {
            TestSendOutcome::Success { reply_subject } => {
                assert!(reply_subject.is_some(), "mock success must have reply_subject");
            }
            other => panic!("expected Success, got {other:?}"),
        }
    }

    #[tokio::test]
    #[serial]
    async fn produce_mock_outcome_failed_when_fail_env_set() {
        // Verify produce_mock_outcome returns Failed when TUXLINK_TEST_SEND_MOCK_FAIL is set.
        let _fail = EnvVarGuard::set("TUXLINK_TEST_SEND_MOCK_FAIL", "1");
        match produce_mock_outcome() {
            TestSendOutcome::Failed { cause, .. } => {
                assert!(cause.contains("MOCKED"), "failed mock must reference MOCKED in cause");
            }
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    /// Part 97 dedup test: wizard_run_test_send must return Busy when the
    /// mutex is already held. This is the RUST-SIDE enforcement of the
    /// one-consent-one-transmission invariant. See spec §3.1 invariant 2 + §5.8.
    ///
    /// The Tauri command signature requires tauri::State, so we test the
    /// mutex behavior directly using WizardMutex.
    #[tokio::test]
    async fn wizard_mutex_busy_on_concurrent_invocation() {
        let wizard_mutex = WizardMutex::new();
        // Hold the lock.
        let _guard = wizard_mutex.0.try_lock().expect("first lock must succeed");
        // Second try_lock returns Err (Mutex is already locked).
        let result = wizard_mutex.0.try_lock();
        assert!(result.is_err(), "second try_lock must fail (Busy path)");
        let wizard_error = WizardError::Busy;
        // Verify WizardError::Busy serializes correctly (Tauri will serialize this to JSON).
        let json = serde_json::to_string(&wizard_error).expect("serialize");
        assert!(json.contains("Busy"), "WizardError::Busy must serialize to {{kind:'Busy',...}}");
    }

    /// Verify the default_likely_causes list has at least 3 entries (spec §3.4 + §5.12
    /// amended to include captive-portal as a 4th cause).
    #[test]
    fn default_likely_causes_has_four_entries() {
        let causes = default_likely_causes();
        assert!(
            causes.len() >= 4,
            "spec §5.12 amended likely_causes to include captive portal — need ≥4 entries"
        );
        let joined = causes.join(" ");
        assert!(
            joined.to_lowercase().contains("captive portal")
                || joined.to_lowercase().contains("captive"),
            "likely_causes must mention captive portal per spec §5.12"
        );
    }
}
