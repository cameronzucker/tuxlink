//! Classifier weights acquisition: the persistent download/sideload job, its
//! status surface, and the Tauri commands behind the wizard step and the
//! Elmer-panel gate (tuxlink-13ofm).
//!
//! Operator-decided shape (recorded on the issue, 2026-08-13):
//! - Weights never ship in the installer. Default source = GitHub release
//!   assets on this repository, version-matched to the running app; a URL or
//!   folder override exists regardless.
//! - The download is a first-class persistent job — visible inline in the
//!   wizard with an explicit "continue setup" act, then in the Elmer panel
//!   until ready; survives restart; resumable; desktop notification on
//!   completion. Nothing moves on silently.
//! - Sideload ships only because digest pinning makes it exactly as verified
//!   as the download ([`tuxlink_classify::pins`] — content-based,
//!   transport-irrelevant).
//!
//! Module map: [`job`] is the persisted record, [`acquire`] is the verify-
//! then-install pipeline, and this file is the driver task + state + command
//! surface.

pub mod acquire;
pub mod job;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter as _, Manager as _, State};
use tuxlink_classify::hosting::{Integrity, ModelLocator, ModelStatus};
use tuxlink_classify::pins;

use job::{FailureClass, JobRecord, Phase, Source};

/// The one model this release acquires. The pins table is the authority; this
/// is its primary (and currently only) entry.
pub fn primary_model() -> &'static pins::PinnedModel {
    &pins::PINNED_MODELS[0]
}

/// Live byte progress for the UI. Throttled; the terminal chunk always emits.
pub const PROGRESS_EVENT: &str = "classify-weights:progress";
/// Phase changes: the full status DTO, so every surface (wizard step, Elmer
/// panel) re-renders from one payload without re-invoking.
pub const STATUS_EVENT: &str = "classify-weights:status";

const EMIT_THROTTLE: Duration = Duration::from_millis(400);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressPayload {
    pub file: String,
    pub got: u64,
    pub total: u64,
}

/// The job half of the status DTO.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobDto {
    /// `waiting` | `downloading` | `verifying` | `complete` | `failed`.
    pub state: &'static str,
    /// Waiting reason / failure message. Operator-facing.
    pub detail: Option<String>,
    /// `network` | `source` | `digest-mismatch` | `io` | `cancelled`.
    pub error_class: Option<&'static str>,
    /// File currently moving, when downloading/verifying.
    pub file: Option<String>,
    pub files_done: Vec<String>,
    /// Where the bytes come from, as a display string.
    pub source: String,
    pub started_unix: u64,
    pub updated_unix: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WeightsStatusDto {
    pub model_id: String,
    /// Whole-payload size, for the wizard's up-front "134 MB" line.
    pub total_bytes: u64,
    /// The locator's verdict: usable weights exist somewhere on the search
    /// path.
    pub ready: bool,
    /// Strongest integrity claim for the ready copy:
    /// `digest-pinned` (installed by this pipeline, every byte verified
    /// against the release pins), `size-verified` (manifest byte-lengths
    /// match), or `structure` (files parse but nothing vouches for bytes).
    pub integrity: Option<&'static str>,
    /// Directory of the ready copy.
    pub location: Option<String>,
    /// The locator's one-line summary (names every path searched when
    /// absent — the wizard shows this under "where Tuxlink looked").
    pub summary: String,
    /// The derived default download base for this app version.
    pub default_source: String,
    pub job: Option<JobDto>,
}

struct Inner {
    running: bool,
    cancel: Arc<AtomicBool>,
    record: Option<JobRecord>,
}

/// Managed state. `models_root` is the user-writable destination — the same
/// directory the locator searches second, so an install is immediately
/// visible to the component that answers "is T1 usable".
pub struct WeightsState {
    models_root: Option<PathBuf>,
    inner: Mutex<Inner>,
}

impl WeightsState {
    /// Build at app setup: resolve the writable root and load any persisted
    /// job so a restart renders the job exactly where it stood.
    pub fn init() -> Self {
        let models_root = user_models_root();
        let record = models_root
            .as_deref()
            .and_then(|root| job::load(root).ok().flatten());
        WeightsState {
            models_root,
            inner: Mutex::new(Inner {
                running: false,
                cancel: Arc::new(AtomicBool::new(false)),
                record,
            }),
        }
    }
}

/// `$XDG_DATA_HOME/tuxlink/models`, else `$HOME/.local/share/tuxlink/models`.
/// Deliberately the same derivation as the locator's user root
/// ([`tuxlink_classify::hosting::default_roots`]) — a download that landed
/// anywhere else would be invisible to the thing it exists to feed.
fn user_models_root() -> Option<PathBuf> {
    let xdg = std::env::var("XDG_DATA_HOME").ok();
    let home = std::env::var("HOME").ok();
    let roots = tuxlink_classify::hosting::default_roots(None, xdg.as_deref(), home.as_deref());
    // default_roots ends with the system root; the user root exists only when
    // one of the env vars did.
    roots
        .into_iter()
        .find(|r| r != Path::new("/usr/share/tuxlink/models"))
}

/// The version-matched GitHub release base for this build.
fn default_release_base() -> String {
    format!(
        "https://github.com/cameronzucker/tuxlink/releases/download/v{}",
        env!("CARGO_PKG_VERSION")
    )
}

fn job_dto(record: &JobRecord) -> JobDto {
    let (state, detail, error_class, file) = match &record.phase {
        Phase::Waiting { detail } => ("waiting", Some(detail.clone()), None, None),
        Phase::Downloading { file } => ("downloading", None, None, Some(file.clone())),
        Phase::Verifying { file } => ("verifying", None, None, Some(file.clone())),
        Phase::Complete { .. } => ("complete", None, None, None),
        Phase::Failed { detail, class } => (
            "failed",
            Some(detail.clone()),
            Some(match class {
                FailureClass::Network => "network",
                FailureClass::Source => "source",
                FailureClass::DigestMismatch => "digest-mismatch",
                FailureClass::Io => "io",
                FailureClass::Cancelled => "cancelled",
            }),
            None,
        ),
    };
    JobDto {
        state,
        detail,
        error_class,
        file,
        files_done: record.files_done.clone(),
        source: record.source.describe(),
        started_unix: record.started_unix,
        updated_unix: record.updated_unix,
    }
}

/// Read the provenance stanza this pipeline writes into `manifest.json`.
/// Bounded read; any irregularity simply means "no digest provenance".
fn digest_provenance(dir: &Path) -> bool {
    let path = dir.join(tuxlink_classify::hosting::MANIFEST_FILE);
    let small = std::fs::metadata(&path)
        .map(|m| m.is_file() && m.len() <= tuxlink_classify::hosting::MANIFEST_MAX_BYTES)
        .unwrap_or(false);
    if !small {
        return false;
    }
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
        .map(|doc| doc["verified"]["method"] == "sha256-release-pins")
        .unwrap_or(false)
}

/// Assemble the full status DTO: locator verdict + job record.
pub fn status_snapshot(state: &WeightsState) -> WeightsStatusDto {
    let model = primary_model();
    let located = ModelLocator::from_env().locate(model.model_id);

    let (ready, integrity, location) = match &located {
        ModelStatus::Ready(l) => {
            let integrity = match l.integrity() {
                Integrity::SizeVerified if digest_provenance(l.dir()) => "digest-pinned",
                Integrity::SizeVerified => "size-verified",
                Integrity::StructureOnly => "structure",
            };
            (true, Some(integrity), Some(l.dir().display().to_string()))
        }
        ModelStatus::Absent { .. } => (false, None, None),
    };

    let record = state.inner.lock().expect("weights state lock").record.clone();

    WeightsStatusDto {
        model_id: model.model_id.to_string(),
        total_bytes: model.total_bytes(),
        ready,
        integrity,
        location,
        summary: located.summary(),
        default_source: default_release_base(),
        job: record.as_ref().map(job_dto),
    }
}

/// Persist + cache + broadcast one record mutation. Every phase change the
/// operator should see funnels through here.
fn announce(app: &AppHandle, state: &WeightsState, record: &JobRecord) {
    if let Some(root) = state.models_root.as_deref() {
        if let Err(e) = job::store(root, record) {
            tracing::warn!(target: "classify_weights", "persist job record: {e}");
        }
    }
    state.inner.lock().expect("weights state lock").record = Some(record.clone());
    let _ = app.emit(STATUS_EVENT, &status_snapshot(state));
}

/// The driver: run the pipeline, auto-retrying network failures with backoff
/// until complete, hard-failed, or cancelled. Owns the record for its whole
/// life; every observable change goes through [`announce`].
async fn drive(app: AppHandle, state: Arc<WeightsState>, mut record: JobRecord, cancel: Arc<AtomicBool>) {
    let model = primary_model();
    let models_root = match state.models_root.clone() {
        Some(root) => root,
        None => {
            record.phase = Phase::Failed {
                detail: "no writable data directory (neither XDG_DATA_HOME nor HOME is set)"
                    .to_string(),
                class: FailureClass::Io,
            };
            announce(&app, &state, &record);
            state.inner.lock().expect("weights state lock").running = false;
            return;
        }
    };

    let last_emit: Mutex<Option<Instant>> = Mutex::new(None);
    let progress_app = app.clone();
    let on_progress = move |file: &str, got: u64, total: u64| {
        let mut last = last_emit.lock().expect("emit throttle lock");
        let due = last
            .map(|t| t.elapsed() >= EMIT_THROTTLE)
            .unwrap_or(true)
            || got == total;
        if due {
            *last = Some(Instant::now());
            let _ = progress_app.emit(
                PROGRESS_EVENT,
                &ProgressPayload {
                    file: file.to_string(),
                    got,
                    total,
                },
            );
        }
    };

    let announce_app = app.clone();
    let announce_state = state.clone();
    let announce_cb = move |r: &JobRecord| announce(&announce_app, &announce_state, r);

    let mut attempt: u32 = 0;
    loop {
        let ctx = acquire::RunCtx {
            models_root: &models_root,
            model,
            cancel: &cancel,
            on_progress: &on_progress,
            announce: &announce_cb,
        };
        let phase = acquire::run_once(&ctx, &mut record).await;
        record.phase = phase;
        announce(&app, &state, &record);

        match &record.phase {
            Phase::Complete { .. } => {
                use tauri_plugin_notification::NotificationExt;
                let _ = app
                    .notification()
                    .builder()
                    .title("Classifier model ready")
                    .body(
                        "Tuxlink finished acquiring the on-device classifier model and \
                         verified every file against this release's pins.",
                    )
                    .show();
                break;
            }
            Phase::Failed { class, .. } if class.auto_retryable() && !cancel.load(Ordering::Relaxed) => {
                let wait = acquire::backoff_secs(attempt);
                attempt = attempt.saturating_add(1);
                record.phase = Phase::Waiting {
                    detail: format!("network trouble — retrying in {wait}s (attempt {attempt})"),
                };
                announce(&app, &state, &record);
                // Sleep in 1s slices so cancel lands promptly mid-backoff.
                for _ in 0..wait {
                    if cancel.load(Ordering::Relaxed) {
                        break;
                    }
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
                if cancel.load(Ordering::Relaxed) {
                    record.phase = Phase::Failed {
                        detail: "cancelled".to_string(),
                        class: FailureClass::Cancelled,
                    };
                    announce(&app, &state, &record);
                    break;
                }
            }
            _ => break,
        }
    }

    state.inner.lock().expect("weights state lock").running = false;
}

/// Start a job. Rejects a second concurrent job (the basemap duplicate-start
/// posture): overwriting the live cancel flag would orphan the running task.
fn start(app: &AppHandle, state: &Arc<WeightsState>, source: Source) -> Result<(), String> {
    let record = {
        let mut inner = state.inner.lock().expect("weights state lock");
        if inner.running {
            return Err(
                "a weights job is already running — cancel it first or let it finish".to_string(),
            );
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let record = JobRecord::new(primary_model().model_id, source, now);
        inner.running = true;
        inner.cancel = Arc::new(AtomicBool::new(false));
        inner.record = Some(record.clone());
        record
    };
    let cancel = state.inner.lock().expect("weights state lock").cancel.clone();
    announce(app, state, &record);
    tauri::async_runtime::spawn(drive(app.clone(), state.clone(), record, cancel));
    Ok(())
}

/// Re-arm a persisted in-flight job at app boot. The original start was the
/// operator's consent; resuming is that same act continuing — and the job is
/// immediately visible in the panel, so nothing moves on silently.
pub fn resume_on_boot(app: &AppHandle) {
    let state: State<'_, Arc<WeightsState>> = app.state();
    let state = state.inner().clone();
    let record = {
        let inner = state.inner.lock().expect("weights state lock");
        match &inner.record {
            Some(r) if r.phase.resumable_on_boot() && !inner.running => Some(r.clone()),
            _ => None,
        }
    };
    if let Some(mut record) = record {
        {
            let mut inner = state.inner.lock().expect("weights state lock");
            inner.running = true;
            inner.cancel = Arc::new(AtomicBool::new(false));
        }
        // An app upgrade may have changed the pins; the skip-list must not
        // outlive the release that vouched for it.
        record.rebase_onto_release();
        record.phase = Phase::Waiting {
            detail: "resuming after restart".to_string(),
        };
        let cancel = state.inner.lock().expect("weights state lock").cancel.clone();
        announce(app, &state, &record);
        tauri::async_runtime::spawn(drive(app.clone(), state, record, cancel));
    }
}

/// Validate an operator-supplied download base URL with the same trust
/// boundary as Elmer endpoints (qe6ie): https for remote hosts, http only on
/// loopback, link-local/metadata and credentials-in-URL always refused. The
/// pins make transport integrity-irrelevant; this boundary is consistency,
/// not the mechanism.
fn validate_base_url(raw: &str) -> Result<String, String> {
    let url = tuxlink_agent_frontend::endpoint::AgentEndpoint::parse(raw)
        .map_err(|e| format!("source URL refused: {e}"))?;
    Ok(url.0.to_string())
}

/// Why a start request was refused — kept distinct so the MCP boundary can
/// classify honestly: a bad URL is the CALLER's to fix; a busy job is not
/// (the disposition-vocabulary rule from tuxlink-2tdmi).
#[derive(Debug)]
pub enum StartError {
    InvalidSource(String),
    Busy(String),
}

impl StartError {
    pub fn message(&self) -> &str {
        match self {
            StartError::InvalidSource(m) | StartError::Busy(m) => m,
        }
    }
}

/// Shared start body for the Tauri command and the MCP port.
pub fn try_download_start(
    source_url: Option<String>,
    app: &AppHandle,
    state: &Arc<WeightsState>,
) -> Result<WeightsStatusDto, StartError> {
    let base_url = match source_url.as_deref().map(str::trim) {
        Some(raw) if !raw.is_empty() => {
            validate_base_url(raw).map_err(StartError::InvalidSource)?
        }
        _ => default_release_base(),
    };
    start(app, state, Source::Release { base_url }).map_err(StartError::Busy)?;
    Ok(status_snapshot(state))
}

// ---- Tauri commands ----

#[tauri::command]
pub fn classify_weights_status(state: State<'_, Arc<WeightsState>>) -> WeightsStatusDto {
    status_snapshot(&state)
}

/// Start (or restart) the download. `source_url` overrides the
/// version-matched release base; the override persists in the job record, so
/// a restart resumes from the same source.
#[tauri::command]
pub fn classify_weights_download_start(
    source_url: Option<String>,
    app: AppHandle,
    state: State<'_, Arc<WeightsState>>,
) -> Result<WeightsStatusDto, String> {
    try_download_start(source_url, &app, &state).map_err(|e| e.message().to_string())
}

/// Cancel the running job. Partial files stay on disk — they are the resume
/// point, and the pins guarantee they can never be mistaken for installed
/// weights.
#[tauri::command]
pub fn classify_weights_download_cancel(
    state: State<'_, Arc<WeightsState>>,
) -> WeightsStatusDto {
    {
        let inner = state.inner.lock().expect("weights state lock");
        if inner.running {
            inner.cancel.store(true, Ordering::Relaxed);
        }
    }
    status_snapshot(&state)
}

/// Install from a local folder (USB stick, LAN mount). Same pipeline, same
/// digest pins, same refusal-by-name as the download — that identity is the
/// whole security argument for sideload existing.
#[tauri::command]
pub fn classify_weights_sideload_import(
    dir: String,
    app: AppHandle,
    state: State<'_, Arc<WeightsState>>,
) -> Result<WeightsStatusDto, String> {
    let dir = PathBuf::from(dir.trim());
    if !dir.is_dir() {
        return Err(format!("{} is not a folder", dir.display()));
    }
    // Refuse importing the install destination into itself.
    if let Some(root) = state.models_root.as_deref() {
        let dest = root.join(primary_model().model_id);
        let canon_src = dir.canonicalize().unwrap_or_else(|_| dir.clone());
        let canon_dest = dest.canonicalize().unwrap_or(dest);
        if canon_src == canon_dest {
            return Err("that folder IS the install location — pick the folder holding the \
                        copies to import"
                .to_string());
        }
    }
    start(&app, &state, Source::Sideload { dir })?;
    Ok(status_snapshot(&state))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_source_is_version_matched_https() {
        let base = default_release_base();
        assert!(base.starts_with("https://github.com/cameronzucker/tuxlink/releases/download/v"));
        assert!(base.ends_with(env!("CARGO_PKG_VERSION")));
    }

    #[test]
    fn base_url_validation_inherits_the_qe6ie_boundary() {
        assert!(validate_base_url("https://github.com/x/y/releases/download/v1").is_ok());
        assert!(validate_base_url("http://127.0.0.1:8080/weights").is_ok());
        // Remote plain http, credentials, and link-local are refused.
        assert!(validate_base_url("http://example.com/weights").is_err());
        assert!(validate_base_url("https://user:pw@example.com/w").is_err());
        assert!(validate_base_url("http://169.254.169.254/latest").is_err());
    }

    #[test]
    fn user_models_root_matches_the_locator_user_root() {
        // Whatever the env says, the destination must be one of the locator's
        // own roots — otherwise installs would be invisible to it.
        if let Some(root) = user_models_root() {
            let locator = ModelLocator::from_env();
            assert!(
                locator.roots().contains(&root),
                "download destination {root:?} not on the locator search path {:?}",
                locator.roots()
            );
        }
    }

    #[test]
    fn job_dto_maps_every_phase_and_class() {
        let mut rec = JobRecord::new(
            "m",
            Source::Release {
                base_url: "https://x".into(),
            },
            1,
        );
        rec.phase = Phase::Failed {
            detail: "boom".into(),
            class: FailureClass::DigestMismatch,
        };
        let dto = job_dto(&rec);
        assert_eq!(dto.state, "failed");
        assert_eq!(dto.error_class, Some("digest-mismatch"));
        assert_eq!(dto.detail.as_deref(), Some("boom"));

        rec.phase = Phase::Downloading {
            file: "model.safetensors".into(),
        };
        let dto = job_dto(&rec);
        assert_eq!(dto.state, "downloading");
        assert_eq!(dto.file.as_deref(), Some("model.safetensors"));
    }
}
