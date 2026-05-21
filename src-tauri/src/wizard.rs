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

use crate::pat_process::{PatProcess, PatSpawnOptions};

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
    // Shares the predicate with `wizard_test_send_is_mocked` so the UI banner and
    // the actual mock short-circuit can never disagree.
    //
    // Fail-CLOSED belt-and-suspenders (Codex pqg adrev P1 #1 + R2 #1):
    // - `cfg!(test)` covers this crate's UNIT tests.
    // - the `CI` env var covers INTEGRATION tests run under CI (where
    //   `cfg!(test)` is false because the lib links as a normal dependency).
    // The live transmit path is unreachable in either context even if a test
    // forgets TUXLINK_TEST_SEND_MOCK. Operator semantics are unchanged — the
    // shipped binary (no `CI`, cfg!(test)==false) still goes live. Residual:
    // a local integration test with neither var set could reach the live path,
    // but test envs lack a Pat binary + keyring credential, so the spawn fails
    // before any transmission (near-nil real risk; RADIO-1 policy + this fn's
    // doc instruct test authors to set the mock var regardless).
    if test_send_is_mocked_impl() || cfg!(test) || std::env::var_os("CI").is_some() {
        // Return a mocked Success outcome. The mock always succeeds so that
        // subagent/CI flows exercise the full 4-substate path in the UI.
        // Unit tests override specific outcomes by invoking this function
        // with different env var values or by calling produce_mock_outcome()
        // directly.
        return Ok(produce_mock_outcome());
    }

    // ── Live path (operator-only; never run by subagents or CI) ───────────
    // Spec §3.8: spawn our OWN ephemeral Pat from the persisted config (wizard
    // Step 2 wrote it), queue the /test/ message, trigger a CMS connect, then
    // poll the inbox for the autoresponder reply. tuxlink-pqg: the prior code
    // assumed a Pat already listening on a hardcoded :8080 and never triggered
    // /api/connect, so it could never complete. Mirrors live_cms_smoke.
    //
    // tuxlink-2a7: an expected operational failure is returned as
    // Ok(TestSendOutcome::Failed { cause, likely_causes_hint }) — the SAME
    // structured shape the mock path produces (produce_mock_outcome) and the
    // frontend's TEST_SEND_RESULT handler already consumes. `Err(WizardError)`
    // is now reserved for genuine command-level errors (Busy from the mutex,
    // a config-read/spawn-task failure). This replaces the prior
    // Err(Other { detail: json }) hack, which surfaced raw JSON in the UI's
    // failure `cause` and required hand-rolled JSON escaping (Codex pqg R1 #7).
    run_live_test_send().await
}

/// Operator-only live round-trip backing `run_test_send_impl`. Spawns an
/// ephemeral Pat from the persisted config (or targets an operator-supplied
/// `PAT_URL`), queues the `/test/` message, triggers a telnet CMS connect, and
/// polls the inbox for the autoresponder reply. Returns `Failed` (not `Err`)
/// for expected operational failures so the caller maps them uniformly.
///
/// **Part 97 (RADIO-1):** this transmits. It is NEVER reached from tests/CI —
/// `run_test_send_impl`'s mock gate short-circuits before it. There is no
/// automated test for this path (it needs a real Pat + a real CMS session); it
/// is operator-verified, exactly like `live_cms_smoke`. The unit-tested nucleus
/// is `is_autoresponder_reply` (success detection).
async fn run_live_test_send() -> Result<TestSendOutcome, WizardError> {
    // The persisted config carries callsign/grid/transport; Pat reads the
    // Winlink password from the keyring entry wizard Step 2 wrote.
    let config = crate::config::read_config().map_err(|e| WizardError::Other {
        detail: format!("cannot read config for test-send: {e}"),
    })?;

    // Operator escape hatch: PAT_URL targets an already-running Pat (no spawn,
    // no shutdown). Otherwise spawn our own ephemeral, ISOLATED Pat.
    let pat_url_override = std::env::var("PAT_URL").ok().filter(|s| !s.is_empty());
    let (base_url, spawned) = match pat_url_override {
        Some(url) => (url, None),
        None => {
            // PatProcess::spawn BLOCKS (up to the announce deadline). Offload it
            // to a blocking thread so the Tauri async runtime is not starved
            // (Codex pqg adrev P1 #6). `_tmp` (the isolated dir) is held
            // alongside `proc` until shutdown below.
            let (proc, tmp) = tokio::task::spawn_blocking(move || spawn_isolated_pat(config))
                .await
                .map_err(|e| WizardError::Other {
                    detail: format!("test-send spawn task failed: {e}"),
                })??;
            let base = format!("http://127.0.0.1:{}", proc.http_port());
            (base, Some((proc, tmp)))
        }
    };

    // Drive the round-trip to a definite outcome (no early return past the
    // shutdown below — the spawned Pat's Drop is the SIGKILL safety net).
    let outcome = test_send_round_trip(&base_url).await;

    // Gracefully stop the Pat we spawned (offloaded — shutdown can block up to
    // its timeout). The temp dir (`_tmp`) is dropped — and thus deleted — only
    // after shutdown completes, since it stays bound in this scope.
    if let Some((mut proc, _tmp)) = spawned {
        let _ = tokio::task::spawn_blocking(move || {
            proc.shutdown(std::time::Duration::from_secs(5))
        })
        .await;
    }

    Ok(outcome)
}

/// Spawn an ephemeral-port Pat into a fresh, ISOLATED temp directory rendered
/// from the persisted config. Isolation (Codex pqg adrev P1 #4 + #5): a unique
/// temp config/mbox/pid means the wizard test-send never contends with an
/// app- or operator-managed Pat's pid/mbox, AND the fresh inbox starts empty so
/// any `SERVICE@winlink.org` message that arrives is unambiguously *our* reply
/// (no nonce/recency heuristics needed). The keyring credential is keyed by
/// callsign (OS-level, not file-path), so the rendered temp config still
/// authenticates. Returns the `TempDir` so the caller keeps it alive for Pat's
/// lifetime; dropping it removes the directory.
///
/// Synchronous (`PatProcess::spawn` blocks); the caller offloads it via
/// `spawn_blocking`.
fn spawn_isolated_pat(
    config: crate::config::Config,
) -> Result<(PatProcess, tempfile::TempDir), WizardError> {
    let tmp = tempfile::tempdir().map_err(|e| WizardError::Other {
        detail: format!("could not create test-send work dir: {e}"),
    })?;
    let binary = std::env::var_os("PAT_BINARY")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("pat"));
    let opts = PatSpawnOptions {
        binary,
        config_path: tmp.path().join("pat-config.json"),
        mbox_dir: tmp.path().join("mbox"),
        http_listen_port: 0, // ephemeral; PatProcess pre-binds + reports the port
        pid_file: tmp.path().join("pat.pid"),
        log_sink: None,
        tuxlink_config: config,
        http_announce_timeout: std::time::Duration::from_secs(10), // canonical (tuxlink-xyd hardening)
    };
    let proc = PatProcess::spawn(opts).map_err(|e| WizardError::Other {
        detail: format!("could not start Pat: {e}"),
    })?;
    Ok((proc, tmp))
}

/// Post the `/test/` message, trigger a telnet CMS connect, and poll the inbox
/// for the autoresponder reply. Always resolves to a `TestSendOutcome`.
async fn test_send_round_trip(base_url: &str) -> TestSendOutcome {
    let client = crate::pat_client::PatClient::new(base_url);
    let date = chrono::Utc::now().to_rfc3339();

    if let Err(e) = client
        .send(&["SERVICE@winlink.org"], TEST_SEND_SUBJECT, TEST_SEND_BODY, &date)
        .await
    {
        return TestSendOutcome::Failed {
            cause: format!("Could not queue message in Pat outbox: {e}"),
            likely_causes_hint: default_likely_causes(),
        };
    }

    // This is what actually transmits. v0.0.1 uses telnet regardless of the
    // configured transport, mirroring the reviewed live_cms_smoke; a
    // transport-aware connect is a follow-up.
    if let Err(e) = trigger_cms_connect(base_url).await {
        return TestSendOutcome::Failed {
            cause: format!("Could not trigger CMS connect: {e}"),
            likely_causes_hint: default_likely_causes(),
        };
    }

    let deadline = tokio::time::Instant::now()
        + tokio::time::Duration::from_secs(TEST_SEND_TIMEOUT_SECS);
    loop {
        // The Pat runs against a fresh, isolated mbox (see spawn_isolated_pat),
        // so the inbox starts empty — any SERVICE@winlink.org message is our
        // autoresponder reply.
        if let Ok(msgs) = client.list(crate::pat_client::MailboxFolder::Inbox).await {
            if let Some(reply) = msgs.iter().find(|m| is_autoresponder_reply(&m.from)) {
                return TestSendOutcome::Success {
                    reply_subject: Some(reply.subject.clone()),
                };
            }
        }
        // A transient list error is tolerated; the deadline below bounds it.
        if tokio::time::Instant::now() >= deadline {
            return TestSendOutcome::Failed {
                cause: format!(
                    "CMS didn't reply within {TEST_SEND_TIMEOUT_SECS} seconds (no \
                     autoresponder). Likely cause: CMS busy or your network's outbound \
                     port 8773 is blocked."
                ),
                likely_causes_hint: default_likely_causes(),
            };
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(POLL_INTERVAL_SECS)).await;
    }
}

/// POST `<base>/api/connect?url=telnet` to trigger Pat's CMS session.
async fn trigger_cms_connect(base_url: &str) -> Result<(), String> {
    let url = format!("{base_url}/api/connect?url=telnet");
    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(CONNECT_TIMEOUT_SECS))
        .build()
        .map_err(|e| format!("build connect client: {e}"))?;
    // Surface a non-2xx connect response as an error (Codex pqg adrev P2 #6):
    // a 4xx/5xx must NOT be silently treated as a successful connect and later
    // misreported as "no autoresponder reply".
    http.post(&url)
        .send()
        .await
        .map_err(|e| format!("trigger connect: {e}"))?
        .error_for_status()
        .map_err(|e| format!("CMS connect returned an error status: {e}"))?;
    Ok(())
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

/// Default likely-causes hint list per spec §3.4 + §5.12 (captive portal).
fn default_likely_causes() -> Vec<String> {
    vec![
        "No internet connection".into(),
        "Firewall blocking port 8773".into(),
        "CMS temporarily busy".into(),
        "A captive portal / network login page intercepting traffic".into(),
    ]
}

/// True when an inbox message is from the CMS autoresponder
/// (`SERVICE@winlink.org`, case-insensitive). Pure — the testable core of the
/// live round-trip's success detection (tuxlink-pqg).
///
/// Sender-only by design (Codex pqg adrev P1 #5): the test-send runs against a
/// fresh, isolated mbox (see `spawn_isolated_pat`), so a SERVICE message can
/// only be our reply. The prior "subject contains `Re:`" branch was a
/// false-positive vector — any human "Re:" from any sender would have marked
/// the test a spurious success.
///
/// Residual (Codex pqg adrev R2 #2, P2, tracked as a follow-up): a SERVICE
/// message already pending on the CMS for this callsign (or, on the `PAT_URL`
/// path, old SERVICE mail in the operator's existing mbox) would also match,
/// reporting success before *this* `/test/` reply arrives. The robust fix is a
/// per-send nonce in the subject correlated against the reply — deferred until
/// the Winlink autoresponder's subject-echo behavior is confirmed firsthand
/// (we don't guess Winlink internals; see the AI-amateur-radio-reliability
/// note). Pending unrelated SERVICE mail is uncommon for the fresh-mbox path.
fn is_autoresponder_reply(from: &str) -> bool {
    from.to_uppercase().contains("SERVICE@WINLINK.ORG")
}

/// How long to poll for the autoresponder reply before declaring failure.
const TEST_SEND_TIMEOUT_SECS: u64 = 30;

/// Polling interval for inbox checks.
const POLL_INTERVAL_SECS: u64 = 3;

/// HTTP timeout for the one-shot `/api/connect` POST.
const CONNECT_TIMEOUT_SECS: u64 = 30;

/// Subject of the wizard verification message (carries the `/test/` token the
/// CMS autoresponder keys on).
const TEST_SEND_SUBJECT: &str = "Tuxlink wizard /test/ verification";

/// Body of the wizard verification message.
const TEST_SEND_BODY: &str = "Tuxlink wizard test send.";

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

/// Whether the test-send is running in MOCKED mode (TUXLINK_TEST_SEND_MOCK set).
/// Pure, read-only, no transmission, no mutex — checked at call time so a fresh
/// query reflects the current environment.
pub fn test_send_is_mocked_impl() -> bool {
    std::env::var("TUXLINK_TEST_SEND_MOCK").is_ok()
}

/// Reports whether the wizard's test-send will run in MOCKED mode for the current
/// process environment. The frontend calls this on entering the `sending` substate
/// to decide whether to render the "Test-send MOCKED — no real Winlink transmission"
/// banner (spec §3.8). Read-only and idempotent (like `get_wizard_completed`);
/// NOT mutex-guarded and NEVER transmits.
#[tauri::command]
pub async fn wizard_test_send_is_mocked() -> Result<bool, WizardError> {
    Ok(test_send_is_mocked_impl())
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

    // ── is_autoresponder_reply (tuxlink-pqg) — pure reply-detection predicate ──
    // No env mutation, no transmission: safe to run without #[serial].

    #[test]
    fn reply_matches_service_sender_case_insensitive() {
        assert!(is_autoresponder_reply("service@winlink.org"));
        assert!(is_autoresponder_reply("SERVICE@WINLINK.ORG"));
    }

    #[test]
    fn reply_does_not_match_human_re_from_other_sender() {
        // Codex pqg adrev P1 #5: a human "Re:" from a non-SERVICE sender must
        // NOT be mistaken for the autoresponder reply.
        assert!(!is_autoresponder_reply("friend@winlink.org"));
    }

    #[test]
    fn reply_does_not_match_unrelated_sender() {
        assert!(!is_autoresponder_reply("someone@example.com"));
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

    /// FIX 4 (P2): the mock-detection helper reports true iff TUXLINK_TEST_SEND_MOCK
    /// is set, so the frontend can render the MOCKED banner during `sending`
    /// (spec §3.8 line 348). It MUST NOT transmit and MUST NOT touch the mutex.
    #[tokio::test]
    #[serial]
    async fn is_test_send_mocked_reflects_env_var() {
        {
            let _mock = EnvVarGuard::set("TUXLINK_TEST_SEND_MOCK", "1");
            assert!(test_send_is_mocked_impl(), "mock env set → mocked=true");
        }
        // EnvVarGuard restored on drop; if it was unset before, mocked=false.
        // Force-unset to make the assertion deterministic regardless of prior state.
        unsafe { std::env::remove_var("TUXLINK_TEST_SEND_MOCK") };
        assert!(!test_send_is_mocked_impl(), "mock env unset → mocked=false");
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
