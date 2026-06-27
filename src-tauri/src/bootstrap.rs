//! App-start bootstrap: the decision logic + the `.setup()` worker.
//!
//! bd issue: tuxlink-9phd (P5).
//!
//! Two layers:
//!
//! 1. [`bootstrap_decision`] — a PURE classification of `read_config()`'s result
//!    into a [`BootstrapAction`]. No I/O, no Tauri; unit-tested directly. This
//!    is the spec §3.3 / adrev #14,#15 gate: pre-wizard + offline both stay
//!    "not connected"; a malformed config is an explicit config error (NOT
//!    "not connected"); only `wizard_completed && connect_to_cms` installs
//!    the native backend.
//!
//! 2. [`run`] — the `.setup()` worker that executes the action: spawns a
//!    dedicated `std::thread` which drives the [`BackendState`] phase and
//!    installs the backend. ALL paths are non-fatal — the app always launches.

use std::sync::Arc;

use tauri::{AppHandle, Emitter, Manager};

use crate::app_backend::{BackendPhase, BackendState};
use crate::config::{Config, ConfigReadError};
use crate::session_log::SessionLogState;
use crate::session_log_emit;
use crate::winlink_backend::{LogLevel, LogLine, LogSource, MailboxChangeSink, NativeBackend, ProgressSink, WireSink};

/// What the bootstrap should do, decided purely from `read_config()`'s result.
#[derive(Debug)]
pub enum BootstrapAction {
    /// Leave the backend `NotConfigured` (the "not connected" empty state):
    /// pre-wizard (no config / `NotFound`), wizard still rendering
    /// (`!wizard_completed`), or offline mode (`!connect_to_cms`).
    NotConnected,
    /// A config file exists but is unusable (`Serde`/`Validation`/`Io`). Surface
    /// an explicit config error — do NOT masquerade as "not connected" (adrev
    /// #15). Carries the reason for the ribbon + the synthetic session-log line.
    ConfigError(String),
    /// CMS configured (`wizard_completed && connect_to_cms`): install the native
    /// backend. The `Config` is boxed because it is the largest variant and is
    /// moved into the install path (avoids a large enum + a needless clone).
    Spawn(Box<Config>),
}

/// Classify `read_config()`'s result into a [`BootstrapAction`] (spec §3.3,
/// adrev #14,#15). Pure: no I/O, no side effects — the unit-test seam for the
/// bootstrap's branch selection.
///
/// - `Err(NotFound)` (pre-wizard, no config) → [`BootstrapAction::NotConnected`].
/// - `Err(Serde | Validation | Io)` (config exists but unusable) →
///   [`BootstrapAction::ConfigError`] carrying the error's `Display`.
/// - `Ok(cfg)` with `!wizard_completed` (wizard still rendering, adrev #14) →
///   [`BootstrapAction::NotConnected`].
/// - `Ok(cfg)` with `wizard_completed && !connect_to_cms` (offline mode) →
///   [`BootstrapAction::NotConnected`].
/// - `Ok(cfg)` with `wizard_completed && connect_to_cms` (CMS mode) →
///   [`BootstrapAction::Spawn`].
pub fn bootstrap_decision(cfg: Result<Config, ConfigReadError>) -> BootstrapAction {
    match cfg {
        // Pre-wizard: no config file yet. Not connected; the wizard renders.
        Err(ConfigReadError::NotFound { .. }) => BootstrapAction::NotConnected,
        // A config exists but is unusable. Explicit error, not "not connected"
        // (adrev #15). `Display` carries the path / serde / validation detail.
        Err(e @ (ConfigReadError::Serde { .. }
        | ConfigReadError::Validation { .. }
        | ConfigReadError::Io { .. })) => BootstrapAction::ConfigError(e.to_string()),
        Ok(cfg) => {
            if !cfg.wizard_completed {
                // The wizard is still rendering (adrev #14): never spawn Pat
                // mid-wizard. Not connected until the wizard writes a completed
                // config.
                BootstrapAction::NotConnected
            } else if !cfg.connect.connect_to_cms {
                // Offline mode: no CMS. Genuinely "not connected".
                BootstrapAction::NotConnected
            } else {
                // CMS mode: install native backend.
                BootstrapAction::Spawn(Box::new(cfg))
            }
        }
    }
}

// ============================================================================
// v1 -> v2 identity migration (Phase 2, tuxlink-7iy2)
// ============================================================================

/// Whether startup must run the one-time v1->v2 identity migration before
/// reading the config. Phase 2 (tuxlink-7iy2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MigrationStep {
    MigrateThenContinue,
    ContinueNoMigration,
    AbortUnsupported,
}

/// Pure mapping from a detected on-disk schema action to the startup step.
pub fn migration_step(action: crate::config::SchemaAction) -> MigrationStep {
    match action {
        crate::config::SchemaAction::MigrateFromV1 => MigrationStep::MigrateThenContinue,
        crate::config::SchemaAction::Current => MigrationStep::ContinueNoMigration,
        // tuxlink-ulrz: additive forward-migration is transparent — read_config loads
        // the older-but-compatible file (new fields default) and the next write
        // re-stamps it to current. No dedicated startup step needed.
        crate::config::SchemaAction::MigrateAdditive => MigrationStep::ContinueNoMigration,
        crate::config::SchemaAction::Unsupported { .. } => MigrationStep::AbortUnsupported,
    }
}

/// One-time v1->v2 identity migration at startup. Reads the on-disk config at
/// `config_path`; if it is a v1 (MigrateFromV1) config, promotes the legacy
/// callsign to the single FULL identity (via IdentityMigration), relocates the
/// flat inbox + tags Sent/Outbox, then rewrites `config_path` at v2 so the
/// subsequent read_config() succeeds. Returns Some(report) iff a migration ran.
/// All-paths-non-fatal at the caller; this returns Err(String) for the caller to
/// log. activation_secret is None at migration time (the operator activates on
/// next launch, Phase 6) — migration does not block on a missing secret.
pub fn migrate_identity_if_v1(
    config_path: &std::path::Path,
    mbox_dir: &std::path::Path,
    store_path: &std::path::Path,
    svc: &crate::identity::IdentityService,
) -> Result<Option<crate::config::MigrationReport>, String> {
    let bytes = match std::fs::read(config_path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None), // fresh install
        Err(e) => return Err(format!("read config {}: {e}", config_path.display())),
    };
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|e| format!("parse config json: {e}"))?;
    let version = value
        .get("schema_version")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;
    match crate::config::detect_schema_action(version) {
        crate::config::SchemaAction::MigrateFromV1 => {}
        // Current or Unsupported: nothing to migrate here.
        _ => return Ok(None),
    }

    // Parse the legacy identity block + the connect flag.
    let v1: crate::config::LegacyConfigV1 = match value.get("identity").cloned() {
        Some(block) => {
            serde_json::from_value(block).map_err(|e| format!("parse legacy identity: {e}"))?
        }
        None => crate::config::LegacyConfigV1 {
            callsign: None,
            identifier: None,
            grid: None,
        },
    };
    let has_cms_account = value
        .get("connect")
        .and_then(|c| c.get("connect_to_cms"))
        .and_then(|b| b.as_bool())
        .unwrap_or(false);

    // tuxlink-6wz3: provision the activation secret from the operator's existing
    // CMS password so the migrated FULL is immediately authenticatable (launch
    // auto-auth + the switcher unlock). Pre-epic builds stored only the CMS
    // password; the activation-secret keyring entry does not exist yet on the
    // first upgrade launch, so read the CMS credential and pass it through.
    // `None` when offline / no callsign / no stored password.
    let activation_secret: Option<String> = v1
        .callsign
        .as_deref()
        .and_then(|c| crate::winlink::credentials::read_password(c).ok());
    let report = crate::config::IdentityMigration::plan(&v1)
        .execute(
            svc,
            mbox_dir,
            store_path,
            has_cms_account,
            activation_secret.as_deref(),
        )
        .map_err(|e| format!("identity migration execute: {e}"))?;

    // Rewrite config at v2 so read_config() succeeds. v1->v2 wire format differs
    // only by schema_version (active_full reads wire-name "callsign"), so bump the
    // version and round-trip through Config to validate the shape, then write.
    let mut bumped = value;
    bumped["schema_version"] = serde_json::json!(crate::config::CONFIG_SCHEMA_VERSION);
    let cfg: crate::config::Config =
        serde_json::from_value(bumped).map_err(|e| format!("v1->v2 config shape: {e}"))?;
    let json =
        serde_json::to_string_pretty(&cfg).map_err(|e| format!("serialize v2 config: {e}"))?;
    // Atomic-ish write to the SAME path (not the global config_path()).
    let parent = config_path
        .parent()
        .ok_or_else(|| "config path has no parent".to_string())?;
    let tmp = tempfile::NamedTempFile::new_in(parent).map_err(|e| format!("tempfile: {e}"))?;
    std::fs::write(tmp.path(), json.as_bytes()).map_err(|e| format!("write tmp: {e}"))?;
    tmp.persist(config_path)
        .map_err(|e| format!("persist config: {e}"))?;

    Ok(Some(report))
}

// ============================================================================
// .setup() bootstrap worker
// ============================================================================

/// Run the app-start bootstrap. Spawns a dedicated `std::thread` and returns
/// IMMEDIATELY so the webview paints without waiting on the backend install —
/// every path inside the thread is non-fatal, so the app ALWAYS launches.
///
/// **AppHandle (adrev #6):** the caller clones the `AppHandle` and moves the
/// clone into the thread; the thread re-enters Tauri only via that owned handle
/// (managed-state lookups, `emit`), never via a borrowed `app`/`State`.
pub fn run(app_handle: AppHandle) {
    std::thread::spawn(move || {
        // Phase 2 (tuxlink-7iy2): one-time v1->v2 identity migration BEFORE
        // read_config (which rejects a v1 schema_version). Non-fatal: on any error
        // we log and fall through to the normal bootstrap with the un-migrated
        // store rather than refuse to launch.
        if let Ok(data_dir) = app_handle.path().app_data_dir() {
            let config_path = crate::config::config_path();
            let mbox_dir = data_dir.join("native-mbox");
            let store_path = crate::config::identity_store_path();
            let svc = crate::identity::IdentityService::new();
            match migrate_identity_if_v1(&config_path, &mbox_dir, &store_path, &svc) {
                Ok(Some(report)) => {
                    tracing::info!(
                        target: "tuxlink::bootstrap",
                        sent_tagged = report.sent_tagged,
                        outbox_tagged = report.outbox_tagged,
                        inbox_moved = report.inbox_moved,
                        was_noop = report.was_noop,
                        "identity migration v1->v2 completed",
                    );
                    emit_backend_line(
                        &app_handle,
                        LogLevel::Info,
                        "Migrated configuration to the multi-identity format.".to_string(),
                    );
                    // Rebuild the search index over the relocated inbox + identity tags.
                    // tuxlink-2ns7: pass the sole FULL so the rebuild indexes the
                    // per-FULL inbox (`mailbox/<FULL>/`); `None` falls back to `_default`.
                    let full = crate::identity::IdentityStore::load(&store_path)
                        .ok()
                        .and_then(|s| s.full().first().map(|f| f.callsign.clone()));
                    if let Some(search) =
                        app_handle.try_state::<crate::search::commands::SearchService>()
                    {
                        if let Err(e) = search.rebuild_index(mbox_dir.clone(), full.as_ref()) {
                            tracing::warn!(
                                target: "tuxlink::bootstrap",
                                error = %e,
                                "post-migration index rebuild failed",
                            );
                        }
                    }
                }
                Ok(None) => {}
                Err(e) => {
                    tracing::warn!(
                        target: "tuxlink::bootstrap",
                        error = %e,
                        "identity migration skipped (non-fatal)",
                    );
                }
            }

            // Phase 4 (tuxlink-2ns7): forward-migrate any legacy mailbox layout
            // (flat `<root>/inbox` OR the old per-FULL scheme `<root>/<CALLSIGN>/inbox`
            // that the now-removed `heal_misplaced_inbox` used to bounce back to flat)
            // into the per-FULL layout `mailbox/<FULL>/...` the production read path
            // now uses. This SUPERSEDES the ej7a heal: instead of bouncing mail back
            // to a flat path, the read side moved to per-FULL, so the migration moves
            // the data forward to match. Each sub-step is idempotent, so it runs
            // safely on every launch. Non-fatal: a failure leaves the install in its
            // current state and logs.
            let sole_full = crate::identity::IdentityStore::load(&store_path)
                .ok()
                .and_then(|s| s.full().first().map(|f| f.callsign.clone()));
            if let Some(full) = sole_full {
                let mbox = crate::native_mailbox::Mailbox::new(&mbox_dir);
                match mbox.migrate_legacy_layout(&full) {
                    Ok(()) => {
                        // Rebuild the index over the per-FULL inbox.
                        if let Some(search) =
                            app_handle.try_state::<crate::search::commands::SearchService>()
                        {
                            if let Err(e) = search.rebuild_index(mbox_dir.clone(), Some(&full)) {
                                tracing::warn!(
                                    target: "tuxlink::bootstrap",
                                    error = %e,
                                    "post-migration index rebuild failed",
                                );
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!(
                            target: "tuxlink::bootstrap",
                            error = %e,
                            "legacy mailbox layout migration skipped (non-fatal)",
                        );
                    }
                }
            }
            // No FULL identity yet (fresh install): nothing to migrate; skip silently.
        }

        let action = bootstrap_decision(crate::config::read_config());
        let state = app_handle.state::<BackendState>();

        tracing::info!(
            target: "tuxlink::bootstrap",
            action = match &action {
                BootstrapAction::NotConnected => "not_connected",
                BootstrapAction::ConfigError(_) => "config_error",
                BootstrapAction::Spawn(_) => "spawn",
            },
            "bootstrap action decided",
        );
        match action {
            // Pre-wizard / wizard-rendering / offline: leave NotConfigured.
            BootstrapAction::NotConnected => {
                state.set_phase(BackendPhase::NotConfigured);
            }
            // Config exists but unusable: explicit ConfigError + one synthetic
            // session-log line carrying the reason (spec §3.3, adrev #15).
            BootstrapAction::ConfigError(reason) => {
                tracing::error!(
                    target: "tuxlink::bootstrap",
                    reason = %reason,
                    "bootstrap config error",
                );
                state.set_phase(BackendPhase::ConfigError {
                    reason: reason.clone(),
                });
                emit_backend_line(&app_handle, LogLevel::Error, reason);
            }
            // CMS mode: install the native Winlink backend.
            BootstrapAction::Spawn(cfg) => {
                install_native(&app_handle, &state, *cfg);
            }
        }
    });
}

/// The CMS-mode install path (native cutover, tuxlink-0ic). Constructs the
/// native Winlink backend over its own on-disk mailbox (`<app_data>/native-mbox`)
/// and installs it — no Pat process, no blocking spawn, no sidecar. Non-fatal: a
/// path-resolver failure surfaces as `Failed` + a session-log line.
///
/// NOTE: the native client presents the SID `tuxlink`, which the production CMS
/// rejects until registered with Winlink (it directs unknown clients to
/// `cms-z.winlink.org`). The backend is installed and the mailbox/compose UI
/// works regardless; a CMS connect against production needs that registration.
/// True when a freshly-persisted config should trigger an IN-SESSION backend
/// install (tuxlink-aw6g). The native backend is otherwise only installed at
/// startup (`run`), and only when the on-disk config already has
/// `wizard_completed && connect_to_cms`. On a fresh install / reinstall,
/// bootstrap runs BEFORE the first-launch wizard (config NotFound →
/// `NotConnected` → no backend), so completing the wizard left the backend
/// offline until the NEXT app restart — `cms_connect` returned "backend
/// offline" and CMS telnet "didn't even start." `wizard_persist_cms` calls this
/// to decide whether to bring the backend online immediately. Guarded on
/// `backend_present` so re-running the wizard against an already-installed
/// backend does not double-spawn. Pure; unit-tested.
pub(crate) fn should_install_after_persist(backend_present: bool, cfg: &Config) -> bool {
    !backend_present && cfg.wizard_completed && cfg.connect.connect_to_cms
}

pub(crate) fn install_native(app_handle: &AppHandle, state: &BackendState, cfg: Config) {
    let mbox_dir = match app_handle.path().app_data_dir() {
        Ok(dir) => dir.join("native-mbox"),
        Err(e) => {
            let reason = format!("could not resolve app data dir for the native mailbox: {e}");
            state.set_phase(BackendPhase::Failed {
                reason: reason.clone(),
            });
            emit_backend_line(app_handle, LogLevel::Error, reason);
            return;
        }
    };

    // Per-step connect progress (tuxlink-gqo): the native connect runs in a
    // blocking task with no `AppHandle`, so it reports each phase through this
    // sink, which appends a `LogSource::Transport` line to the session log (so it
    // survives in the snapshot) and emits it live. Mirrors `emit_backend_line`,
    // but tagged Transport rather than Backend.
    let progress_app = app_handle.clone();
    let progress: ProgressSink = Arc::new(move |msg: &str| {
        let buffer = progress_app.state::<Arc<SessionLogState>>();
        session_log_emit::emit(&progress_app, &buffer, LogLevel::Info, LogSource::Transport, msg);
    });

    // tuxlink-nki: raw B2F wire lines. The native connect tees every on-wire
    // protocol line (both directions) into this sink, which appends a
    // `LogSource::Wire` line to the session log + emits it live — so the operator
    // can watch the real `[WL2K-...]`/`;FW`/`FF`/`FQ` dialogue under "Raw output"
    // (the Human view suppresses wire lines). LogLevel::Trace — verbose detail.
    // Mirrors the progress sink above, tagged Wire rather than Transport.
    let wire_app = app_handle.clone();
    let wire: WireSink = Arc::new(move |msg: &str| {
        let buffer = wire_app.state::<Arc<SessionLogState>>();
        session_log_emit::emit(&wire_app, &buffer, LogLevel::Trace, LogSource::Wire, msg);
    });

    // tuxlink-b2sk: mailbox mutations should reach the shell immediately. The
    // frontend listens for this lightweight event and invalidates the
    // `['mailbox']` query family instead of waiting for the 10s polling interval
    // or for `cms_connect` to return after its connected-state hold.
    let mailbox_app = app_handle.clone();
    let mailbox_change: MailboxChangeSink = Arc::new(move || {
        let _ = mailbox_app.emit("mailbox:changed", ());
    });

    // tuxlink-686: inject the live PositionArbiter so the on-air CMS locator is
    // the arbiter's broadcast_grid() (live + precision-reduced) rather than the
    // stale config snapshot the backend was constructed with. The arbiter is
    // managed state registered in lib.rs::run() above the .setup() call; the Arc
    // ref-count is incremented here, not moved, so the lib.rs binding stays alive.
    let arbiter = (*app_handle.state::<Arc<crate::position::PositionArbiter>>()).clone();

    // Codex adrev fix (find-messages): share the search index Arc with the
    // production mailbox so incremental index hooks run on every
    // store/move_to/mark_read. The SearchService is registered in lib.rs
    // .setup() before bootstrap::run; if it's absent (e.g. SQLite failed to
    // open) the mailbox runs without an index and only rebuild-index works.
    let search_index = app_handle
        .try_state::<crate::search::commands::SearchService>()
        .map(|svc| svc.index.clone());

    // tuxlink-2ns7: resolve the operator's sole FULL identity so the production
    // mailbox's bare store/list/read resolve the per-FULL received-mail subtree
    // (`mailbox/<FULL>/`) — matching where `migrate_legacy_layout` re-homes mail.
    // `None` (no identity yet, fresh install) leaves the mailbox un-defaulted
    // (resolves `_default`).
    // tuxlink-nx3g: EXACTLY one FULL (not `.first()`), so a multi-FULL store does
    // not silently default to / auto-auth / self-heal one arbitrary identity.
    let sole_full = exactly_one_full(&crate::config::identity_store_path());

    let mut backend = NativeBackend::with_progress(cfg, mbox_dir, progress)
        .with_wire_log(wire)
        .with_mailbox_change(mailbox_change)
        .with_position(arbiter);
    if let Some(full) = &sole_full {
        backend = backend.with_default_identity(full);
    }
    if let Some(index) = search_index {
        backend = backend.with_index(index);
    }

    // tuxlink-6wz3: auto-authenticate the configured identity on launch so
    // transmit works without a manual per-launch unlock (WLE parity). The
    // active-identity gate (Phase 3) otherwise blocks every transmit until the
    // operator unlocks via the switcher; for the single configured identity that
    // is friction the operator has not asked for. Reads the stored CMS password
    // and authenticates against the activation secret (kept in sync by
    // write_password / provisioned by migration). Best-effort: any failure
    // (no secret, mismatch, keyring error) leaves the slot empty and the operator
    // can still unlock manually via the switcher. Switching to a DIFFERENT FULL
    // or a tactical still goes through the switcher's explicit unlock.
    if let Some(full) = &sole_full {
        let svc = crate::identity::IdentityService::new();
        match resolve_auto_identity(&svc, full, |c| {
            // tuxlink-nx3g: distinguish "no stored CMS password" (expected; nothing to
            // heal) from a keyring BACKEND failure (locked / unavailable) so the latter
            // is visible in the trace instead of looking like an absent credential.
            // Both still fail closed (return None).
            match crate::winlink::credentials::read_password(c) {
                Ok(pw) => Some(pw),
                Err(crate::winlink::credentials::KeyringError::NoEntry { .. }) => None,
                Err(e @ crate::winlink::credentials::KeyringError::Backend(_)) => {
                    tracing::warn!(
                        target: "tuxlink::bootstrap",
                        error = ?e,
                        callsign = %c,
                        "CMS credential keyring read failed (backend locked/unavailable); auto-auth cannot proceed",
                    );
                    None
                }
            }
        }) {
            AutoAuth::Authenticated(session) => {
                backend.set_active_identity(session);
                tracing::info!(
                    target: "tuxlink::bootstrap",
                    callsign = %full.as_str(),
                    "auto-authenticated active identity from stored credential",
                );
            }
            AutoAuth::Healed(session) => {
                backend.set_active_identity(session);
                // tuxlink-nx3g: auto-provisioning a credential is VISIBLE, not silent.
                emit_backend_line(
                    app_handle,
                    LogLevel::Warn,
                    format!(
                        "Repaired the missing activation secret for {} from your saved \
                         credentials and unlocked transmit.",
                        full.as_str()
                    ),
                );
                tracing::warn!(
                    target: "tuxlink::bootstrap",
                    callsign = %full.as_str(),
                    "self-healed missing activation secret from stored CMS credential",
                );
            }
            AutoAuth::HealFailed => {
                emit_backend_line(
                    app_handle,
                    LogLevel::Warn,
                    format!(
                        "{} could not be unlocked automatically (activation secret missing \
                         and not repairable). Unlock it from the callsign menu, or re-run setup.",
                        full.as_str()
                    ),
                );
                tracing::warn!(
                    target: "tuxlink::bootstrap",
                    callsign = %full.as_str(),
                    "auto-auth self-heal failed (empty stored credential or keyring error)",
                );
            }
            AutoAuth::Unavailable => tracing::warn!(
                target: "tuxlink::bootstrap",
                callsign = %full.as_str(),
                "auto-auth unavailable (no/mismatched stored credential); operator unlocks via the switcher",
            ),
        }
    }

    // 2026-05-31 operator-flagged: the 5s status poll missed sub-second
    // CMS-Z exchanges so the user saw Connecting → Disconnected with no
    // visible Connected state. Subscribe to the backend's status broadcast
    // BEFORE handing the Arc to BackendState (otherwise we lose the racey
    // initial Disconnected). Emit `backend_status:change` with the StatusDto
    // payload the frontend's `useStatus.ts` already understands.
    let mut status_rx = backend.subscribe_status();
    let status_app = app_handle.clone();
    tauri::async_runtime::spawn(async move {
        while let Ok(s) = status_rx.recv().await {
            let dto = crate::ui_commands::StatusDto::from(s);
            let _ = status_app.emit("backend_status:change", &dto);
        }
        // Channel closed = backend dropped. The task exits silently.
    });

    state.install(Arc::new(backend));
    tracing::info!(
        target: "tuxlink::bootstrap",
        "native Winlink backend installed",
    );
    emit_backend_line(
        app_handle,
        LogLevel::Info,
        "Native Winlink backend ready.".to_string(),
    );
}

/// Decide the active identity to auto-establish on launch (tuxlink-6wz3).
///
/// Fetches the stored credential for `full` via the injected `read_pw` and
/// authenticates it against the activation secret. Returns the `SessionIdentity`
/// to install, or `None` (no stored credential, or it does not match the
/// activation secret — the operator unlocks manually via the switcher).
///
/// Factored out of `install_native` (which needs a real `AppHandle` + keyring)
/// so the auto-auth decision is unit-testable headless: inject a memory-keyring
/// `IdentityService` and a closure for the password read. This is the guardrail
/// test seam for the transmit-un-brick (tuxlink-6wz3).
/// Outcome of the launch auto-auth + self-heal attempt (tuxlink-nx3g), so the
/// caller can emit the right VISIBLE session-log line — auto-provisioning a
/// credential must not be silent.
enum AutoAuth {
    /// Authenticated from an existing activation secret (the normal path).
    Authenticated(crate::identity::SessionIdentity),
    /// The activation secret was MISSING and was self-healed from the stored CMS
    /// password (orphan-v2 un-brick); now authenticated.
    Healed(crate::identity::SessionIdentity),
    /// The secret was missing and a heal was attempted but could not complete
    /// (empty stored credential, or a keyring backend error).
    HealFailed,
    /// No usable identity: no stored credential, or a credential mismatch.
    Unavailable,
}

#[cfg(test)]
impl AutoAuth {
    /// Test helper: collapse the outcome to the session (if any). Production code
    /// matches the variants directly (to emit per-variant log lines).
    fn into_session(self) -> Option<crate::identity::SessionIdentity> {
        match self {
            AutoAuth::Authenticated(s) | AutoAuth::Healed(s) => Some(s),
            AutoAuth::HealFailed | AutoAuth::Unavailable => None,
        }
    }
}

fn resolve_auto_identity(
    svc: &crate::identity::IdentityService,
    full: &crate::identity::Callsign,
    read_pw: impl FnOnce(&str) -> Option<String>,
) -> AutoAuth {
    let pw = match read_pw(full.as_str()) {
        Some(pw) => pw,
        None => return AutoAuth::Unavailable, // no stored CMS credential — nothing to do
    };
    match svc.authenticate(full, &pw) {
        Ok(h) => AutoAuth::Authenticated(crate::identity::SessionIdentity::full(h)),
        Err(crate::identity::IdentityError::NoSecretSet) => {
            // Orphan-v2: the activation secret was never written. Heal it from the
            // TRUSTED stored CMS password (read from the keyring — no user input, so
            // no bypass), then retry once. `heal_activation_secret` refuses an empty
            // pw, never overwrites an existing secret, and fails closed on a backend
            // error (tuxlink-nx3g).
            match svc.heal_activation_secret(full, &pw) {
                Ok(true) => match svc.authenticate(full, &pw) {
                    Ok(h) => AutoAuth::Healed(crate::identity::SessionIdentity::full(h)),
                    _ => AutoAuth::HealFailed,
                },
                _ => AutoAuth::HealFailed, // Ok(false) [empty pw] or Err [backend] — could not heal
            }
        }
        Err(_) => AutoAuth::Unavailable, // CredentialMismatch / Keyring — never auto-heal
    }
}

/// The SOLE FULL identity in the store, or `None` if there are zero or MORE than
/// one (tuxlink-nx3g). A multi-FULL store must not silently default to, auto-auth,
/// or self-heal one arbitrary FULL — the operator picks via the switcher. (The
/// prior `.first()` mis-scoped BOTH the mailbox default and the auto-auth.)
fn exactly_one_full(store_path: &std::path::Path) -> Option<crate::identity::Callsign> {
    let store = crate::identity::IdentityStore::load(store_path).ok()?;
    let fulls = store.full();
    if fulls.len() == 1 {
        Some(fulls[0].callsign.clone())
    } else {
        None
    }
}

/// One iteration of the buffer-polling drain: emit every buffered line with
/// `seq > last_seq` (oldest first), advancing the cursor past each. Returns the
/// updated cursor (the max `seq` emitted, or `last_seq` unchanged if nothing was
/// newer). `emit` receives each line in seq order exactly once.
///
/// Pure w.r.t. the cursor logic (the side effect is the injected `emit`), so the
/// "emit each new line once, never re-emit, advance monotonically" contract is
/// unit-tested without a Tauri runtime — see `tests::drain_step_*`.
///
/// Currently only consumed by unit tests; the production caller (`start_drain`)
/// was removed in tuxlink-9phd P5 when native logging stopped using the
/// broadcast-based drain. Retained because the test seam is the value.
#[cfg_attr(not(test), allow(dead_code))]
fn drain_step(
    buffer: &SessionLogState,
    last_seq: u64,
    mut emit: impl FnMut(LogLine),
) -> u64 {
    let mut cursor = last_seq;
    for line in buffer.snapshot_since(last_seq) {
        cursor = line.seq;
        emit(line);
    }
    cursor
}

/// Append a synthetic `LogSource::Backend` line to the durable buffer (so it
/// survives in `session_log_snapshot`) AND emit it live on `session_log:line`
/// (so an already-listening UI sees it immediately). Used for the bootstrap's
/// own error / config-error lines (spec §3.3, §5). Best-effort: a poisoned
/// buffer lock (append → seq 0) or an emit error is swallowed — the phase
/// transition is the primary signal; the log line is the explanatory detail.
fn emit_backend_line(app_handle: &AppHandle, level: LogLevel, message: String) {
    if !bootstrap_line_visible_in_session_log(level) {
        return;
    }

    let buffer = app_handle.state::<Arc<SessionLogState>>();
    session_log_emit::emit(app_handle, &buffer, level, LogSource::Backend, message);
}

fn bootstrap_line_visible_in_session_log(level: LogLevel) -> bool {
    matches!(level, LogLevel::Warn | LogLevel::Error)
}

#[cfg(test)]
mod tests {
    use super::*;

    // tuxlink-6wz3 guardrail: the launch auto-auth decision. Establishes the
    // active identity ONLY when the stored credential matches the activation
    // secret; otherwise None (operator unlocks manually). This is the headless
    // seam for the transmit-un-brick — it would have caught the "no active
    // identity on a fresh install" brick that shipped.
    #[test]
    fn resolve_auto_identity_matches_stored_credential_else_none() {
        let svc = crate::identity::IdentityService::with_memory_keyring();
        let call = crate::identity::Callsign::parse("W1ABC").unwrap();
        svc.set_activation_secret(&call, "secret-pw").unwrap();

        // Matching stored credential → active identity established.
        let session = resolve_auto_identity(&svc, &call, |_| Some("secret-pw".to_string())).into_session();
        assert!(session.is_some(), "matching credential must auto-authenticate");
        assert_eq!(session.unwrap().mycall().as_str(), "W1ABC");

        // Wrong credential → None (no false unlock).
        assert!(
            resolve_auto_identity(&svc, &call, |_| Some("wrong".to_string())).into_session().is_none(),
            "a mismatched credential must NOT auto-authenticate"
        );

        // No stored credential → None.
        assert!(
            resolve_auto_identity(&svc, &call, |_| None).into_session().is_none(),
            "absent credential must NOT auto-authenticate"
        );
    }

    #[test]
    fn resolve_auto_identity_self_heals_an_orphan_v2_missing_secret() {
        // tuxlink-nx3g: orphan-v2 — a stored CMS password but NO activation secret.
        // Auto-auth must heal the secret from the CMS password and authenticate.
        let svc = crate::identity::IdentityService::with_memory_keyring();
        let call = crate::identity::Callsign::parse("W1ABC").unwrap();
        // No set_activation_secret -> authenticate would be NoSecretSet.
        let outcome = resolve_auto_identity(&svc, &call, |_| Some("cms-pw".to_string()));
        assert!(matches!(outcome, AutoAuth::Healed(_)), "missing secret must self-heal");
        // The secret is now provisioned: a second launch authenticates normally.
        assert!(matches!(
            resolve_auto_identity(&svc, &call, |_| Some("cms-pw".to_string())),
            AutoAuth::Authenticated(_)
        ), "after heal, the next launch authenticates from the now-present secret");
    }

    #[test]
    fn resolve_auto_identity_does_not_heal_without_a_stored_cms_pw() {
        let svc = crate::identity::IdentityService::with_memory_keyring();
        let call = crate::identity::Callsign::parse("W1ABC").unwrap();
        // NoSecretSet AND no CMS pw to copy from -> Unavailable (nothing to heal).
        assert!(matches!(
            resolve_auto_identity(&svc, &call, |_| None),
            AutoAuth::Unavailable
        ));
    }

    #[test]
    fn exactly_one_full_is_some_only_for_a_single_full() {
        use crate::identity::{Callsign, FullIdentity, IdentityStore};
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("identity-store.json");
        let mk = |c: &str| FullIdentity {
            callsign: Callsign::parse(c).unwrap(),
            label: None,
            has_cms_account: true,
            cms_registered: true,
        };

        // Missing store / zero FULLs -> None.
        assert!(exactly_one_full(&path).is_none(), "no store / zero FULLs -> None");

        // One FULL -> Some(that call).
        let mut store = IdentityStore::load(&path).unwrap();
        store.add_full(mk("W1ABC")).unwrap();
        store.save().unwrap();
        assert_eq!(
            exactly_one_full(&path).map(|c| c.as_str().to_string()),
            Some("W1ABC".to_string())
        );

        // Two FULLs -> None (no auto-default / auto-auth / auto-heal of an arbitrary one).
        let mut store = IdentityStore::load(&path).unwrap();
        store.add_full(mk("KK7XYZ")).unwrap();
        store.save().unwrap();
        assert!(exactly_one_full(&path).is_none(), "multi-FULL must NOT resolve a sole identity");
    }

    use crate::config::{
        CmsTransport, Config, ConfigReadError, ConfigValidationError, ConnectConfig, GpsState,
        IdentityConfig, PacketConfig, PositionPrecision, PositionSource, PrivacyConfig,
        CONFIG_SCHEMA_VERSION,
    };

    /// CMS-mode config fixture (`wizard_completed = true`, `connect_to_cms =
    /// true`). Built like the `ui_commands` config tests.
    #[allow(deprecated)] // sets pat_mbo_address on Config literal; field deprecated per tuxlink-9phd T8.1
    fn cms_config() -> Config {
        Config {
            schema_version: CONFIG_SCHEMA_VERSION,
            wizard_completed: true,
            connect: ConnectConfig {
                connect_to_cms: true,
                transport: CmsTransport::CmsSsl,
                host: crate::config::default_cms_host(),
            },
            identity: IdentityConfig {
                active_full: Some("W4PHS".into()),
                identifier: None,
                grid: Some("EM10ab".into()),
            },
            privacy: PrivacyConfig {
                gps_state: GpsState::BroadcastAtPrecision,
                position_precision: PositionPrecision::FourCharGrid,
                position_source: PositionSource::Gps,
            },
            pat_mbo_address: None,
            packet: PacketConfig::default(),
            modem_ardop: None,
            modem_vara: None,
            rig: crate::config::RigUiConfig::default(),
            telnet_listen: crate::config::TelnetListenUiConfig::default(),
            network_po_favorites: Vec::new(),
            review_inbound_before_download: false,
            map_tile_source: None,
            aredn_master_node_host: None,
            aprs: crate::config::AprsConfig::default(),
            trash_auto_purge: true,
            trash_retention_days: 30,
            close_to_tray: true,
            close_prompt_seen: false,
        }
    }

    // Err(NotFound) — pre-wizard, no config file → NotConnected.
    #[test]
    fn not_found_is_not_connected() {
        let action = bootstrap_decision(Err(ConfigReadError::NotFound {
            path: "/nonexistent/config.json".into(),
        }));
        assert!(matches!(action, BootstrapAction::NotConnected));
    }

    // Err(Serde) — config exists but won't parse → ConfigError(..).
    #[test]
    fn serde_error_is_config_error() {
        let serde_err = serde_json::from_str::<Config>("{ not json").unwrap_err();
        let action = bootstrap_decision(Err(ConfigReadError::Serde { source: serde_err }));
        match action {
            BootstrapAction::ConfigError(reason) => {
                assert!(!reason.is_empty(), "ConfigError carries a non-empty reason");
            }
            other => panic!("expected ConfigError, got {other:?}"),
        }
    }

    // Err(Validation) — config parsed but failed semantic validation →
    // ConfigError(..).
    #[test]
    fn validation_error_is_config_error() {
        let action = bootstrap_decision(Err(ConfigReadError::Validation {
            source: ConfigValidationError::CmsPathNoActiveFull,
        }));
        match action {
            BootstrapAction::ConfigError(reason) => {
                assert!(reason.contains("FULL"), "reason mentions the validation cause");
            }
            other => panic!("expected ConfigError, got {other:?}"),
        }
    }

    // Err(Io) — config path unreadable (not NotFound) → ConfigError(..).
    #[test]
    fn io_error_is_config_error() {
        let action = bootstrap_decision(Err(ConfigReadError::Io {
            path: "/some/config.json".into(),
            source: std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied"),
        }));
        assert!(matches!(action, BootstrapAction::ConfigError(_)));
    }

    // Ok(cfg) with !wizard_completed — the wizard is still rendering (adrev
    // #14) → NotConnected (never install backend mid-wizard).
    #[test]
    fn wizard_incomplete_is_not_connected() {
        let mut cfg = cms_config();
        cfg.wizard_completed = false;
        let action = bootstrap_decision(Ok(cfg));
        assert!(matches!(action, BootstrapAction::NotConnected));
    }

    // Ok(cfg) with wizard_completed && !connect_to_cms — offline mode →
    // NotConnected.
    #[test]
    fn offline_mode_is_not_connected() {
        let mut cfg = cms_config();
        cfg.connect.connect_to_cms = false;
        // Offline config forbids a callsign (Config::validate), but
        // bootstrap_decision does not re-validate — it only reads the two
        // gating flags. Clear callsign anyway to keep the fixture coherent.
        cfg.identity.active_full = None;
        let action = bootstrap_decision(Ok(cfg));
        assert!(matches!(action, BootstrapAction::NotConnected));
    }

    // Ok(cfg) with wizard_completed && connect_to_cms — CMS mode → Spawn.
    #[test]
    fn cms_mode_is_spawn() {
        let action = bootstrap_decision(Ok(cms_config()));
        match action {
            BootstrapAction::Spawn(cfg) => {
                assert!(cfg.connect.connect_to_cms);
                assert!(cfg.wizard_completed);
            }
            other => panic!("expected Spawn, got {other:?}"),
        }
    }

    #[test]
    fn bootstrap_session_log_lines_are_problem_only() {
        assert!(!bootstrap_line_visible_in_session_log(LogLevel::Info));
        assert!(!bootstrap_line_visible_in_session_log(LogLevel::Debug));
        assert!(!bootstrap_line_visible_in_session_log(LogLevel::Trace));
        assert!(bootstrap_line_visible_in_session_log(LogLevel::Warn));
        assert!(bootstrap_line_visible_in_session_log(LogLevel::Error));
    }

    // ========================================================================
    // v1 -> v2 identity migration (Phase 2, tuxlink-7iy2)
    // ========================================================================

    #[test]
    fn startup_runs_migration_for_v1_then_spawns() {
        use crate::config::SchemaAction;
        assert_eq!(
            super::migration_step(SchemaAction::MigrateFromV1),
            super::MigrationStep::MigrateThenContinue
        );
        assert_eq!(
            super::migration_step(SchemaAction::Current),
            super::MigrationStep::ContinueNoMigration
        );
        assert_eq!(
            super::migration_step(SchemaAction::Unsupported { found: 9 }),
            super::MigrationStep::AbortUnsupported
        );
    }

    #[test]
    fn migrate_identity_if_v1_promotes_callsign_and_rewrites_config_v2() {
        use crate::native_mailbox::Mailbox;
        use crate::winlink_backend::MailboxFolder;

        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        let mbox_dir = dir.path().join("native-mbox");
        let store_path = dir.path().join("identities.json");

        // A v1 CMS config on disk (schema_version 1, identity.callsign set).
        std::fs::write(
            &config_path,
            br#"{
            "schema_version": 1, "wizard_completed": true,
            "connect": {"connect_to_cms": true, "transport": "CmsSsl"},
            "identity": {"callsign": "W1ABC", "identifier": null, "grid": "CN87"},
            "privacy": {"gps_state": "Off", "position_precision": "FourCharGrid"},
            "pat_mbo_address": null
        }"#,
        )
        .unwrap();

        // A seeded flat inbox message.
        let mbox = Mailbox::new(&mbox_dir);
        let inbox_id = mbox
            .store(
                MailboxFolder::Inbox,
                &crate::winlink::compose::compose_message("N7CPZ", &["W1AW"], &[], "M", "b", 1_716_200_000)
                    .to_bytes(),
            )
            .unwrap();

        let svc = crate::identity::IdentityService::with_memory_keyring();
        let report = super::migrate_identity_if_v1(&config_path, &mbox_dir, &store_path, &svc)
            .expect("migration ok")
            .expect("a migration ran");
        assert!(!report.was_noop);

        // Store has the one FULL.
        let store = crate::identity::IdentityStore::load(&store_path).unwrap();
        assert_eq!(store.full().len(), 1);
        assert_eq!(store.full()[0].callsign.as_str(), "W1ABC");

        // Config rewritten to v2 and now read_config-shaped: parse it as Config.
        let bytes = std::fs::read(&config_path).unwrap();
        let cfg: crate::config::Config =
            serde_json::from_slice(&bytes).expect("rewritten config is valid v2");
        assert_eq!(cfg.schema_version, crate::config::CONFIG_SCHEMA_VERSION);
        assert_eq!(cfg.identity.active_full.as_deref(), Some("W1ABC"));

        // tuxlink-ej7a: the inbox stays FLAT and visible to the production read
        // path; the migration must NOT relocate it under a per-FULL root.
        let production_mbox = Mailbox::new(&mbox_dir);
        let metas = production_mbox.list(MailboxFolder::Inbox).unwrap();
        assert_eq!(metas.len(), 1, "inbox still visible to the flat production read path");
        assert_eq!(metas[0].id, inbox_id);
        assert!(!mbox_dir.join("W1ABC").exists(), "no per-FULL inbox dir created by v1->v2");
        assert!(!report.inbox_moved);

        // Idempotent: a second run no-ops (store already has a FULL).
        let again =
            super::migrate_identity_if_v1(&config_path, &mbox_dir, &store_path, &svc).unwrap();
        assert!(
            again.is_none() || again.unwrap().was_noop,
            "second startup migration is a no-op"
        );
    }

    // tuxlink-2ns7 Phase 4 wiring: the per-FULL forward-migration (which
    // supersedes the removed ej7a heal) is unit-tested at the source in
    // `native_mailbox::migrate_legacy_layout` (idempotency + old-scheme rescue).
    // bootstrap::run() wires it; the wiring itself has no Tauri-free unit seam.

    // ========================================================================
    // drain_step: buffer-polling cursor logic
    // The drain emits EVERY buffered line exactly once, in seq order, advancing
    // a monotonic cursor. Tested via a closure sink so no Tauri runtime is
    // needed.
    // ========================================================================

    fn log_line(msg: &str) -> LogLine {
        LogLine {
            seq: 0, // append() assigns the real seq
            timestamp_iso: "2026-05-20T00:00:00Z".into(),
            level: LogLevel::Info,
            source: LogSource::Backend,
            message: msg.into(),
        }
    }

    // A first poll from cursor 0 emits ALL buffered lines in seq order, and
    // advances the cursor to the last seq.
    #[test]
    fn drain_step_first_poll_emits_all_buffered_lines_in_seq_order() {
        let buf = SessionLogState::new(16);
        for m in ["startup-a", "startup-b", "startup-c"] {
            buf.append(log_line(m));
        }
        let mut emitted: Vec<(u64, String)> = Vec::new();
        let new_cursor = drain_step(&buf, 0, |l| emitted.push((l.seq, l.message)));

        assert_eq!(
            emitted,
            vec![
                (1, "startup-a".to_string()),
                (2, "startup-b".to_string()),
                (3, "startup-c".to_string()),
            ],
            "every pre-existing line is emitted once, oldest-first"
        );
        assert_eq!(new_cursor, 3, "cursor advances to the last emitted seq");
    }

    // A subsequent poll emits only lines newer than the cursor (no re-emit), and
    // a poll with nothing new leaves the cursor unchanged and emits nothing.
    #[test]
    fn drain_step_advances_cursor_and_never_reemits() {
        let buf = SessionLogState::new(16);
        for m in ["a", "b"] {
            buf.append(log_line(m));
        }
        let mut first: Vec<u64> = Vec::new();
        let cursor = drain_step(&buf, 0, |l| first.push(l.seq));
        assert_eq!(first, vec![1, 2]);
        assert_eq!(cursor, 2);

        // Nothing new: empty emit, cursor unchanged.
        let mut empty: Vec<u64> = Vec::new();
        let cursor = drain_step(&buf, cursor, |l| empty.push(l.seq));
        assert!(empty.is_empty(), "no re-emit when nothing is newer than the cursor");
        assert_eq!(cursor, 2, "cursor unchanged when nothing newer");

        // Append more; next poll emits only the new ones.
        for m in ["c", "d"] {
            buf.append(log_line(m));
        }
        let mut next: Vec<u64> = Vec::new();
        let cursor = drain_step(&buf, cursor, |l| next.push(l.seq));
        assert_eq!(next, vec![3, 4], "only lines newer than the cursor are emitted");
        assert_eq!(cursor, 4);
    }

    // tuxlink-aw6g: the in-session post-wizard backend-install decision. This is
    // the regression guard for "CMS telnet dead until restart after a fresh
    // install" — the wizard must bring the backend online when (and only when)
    // it isn't already present and the config is CMS-mode.
    #[test]
    fn install_after_persist_true_when_cms_and_no_backend() {
        assert!(should_install_after_persist(false, &cms_config()));
    }

    #[test]
    fn install_after_persist_false_when_backend_already_present() {
        assert!(!should_install_after_persist(true, &cms_config()));
    }

    #[test]
    fn install_after_persist_false_for_offline_config() {
        let mut cfg = cms_config();
        cfg.connect.connect_to_cms = false;
        assert!(!should_install_after_persist(false, &cfg));
    }

    #[test]
    fn install_after_persist_false_when_wizard_incomplete() {
        let mut cfg = cms_config();
        cfg.wizard_completed = false;
        assert!(!should_install_after_persist(false, &cfg));
    }
}
