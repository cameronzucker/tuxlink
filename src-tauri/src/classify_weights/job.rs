//! The weights-acquisition job record: what phase the one job is in, where it
//! is getting bytes from, and how that survives an app restart
//! (tuxlink-13ofm).
//!
//! Design rule from the operator: the download is a first-class persistent
//! JOB — it survives restart, resumes, and renders wherever the user actually
//! looks (wizard step inline, Elmer panel afterwards). Nothing moves on
//! silently. This module is the record half of that promise: a small JSON
//! file at `<models-root>/.weights-job.json`, rewritten atomically on every
//! PHASE change (byte-level progress is not persisted here — the `.part`
//! file's own length is the resume point, so the record never churns during
//! a transfer).
//!
//! The record deliberately stores no digests: expected content lives in
//! [`tuxlink_classify::pins`] (the release-pinned table), and a record that
//! restated them would be a second, forgeable source.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// File name of the persisted record, directly under the models root. The
/// leading dot keeps it out of any model directory the locator inspects.
pub const JOB_FILE: &str = ".weights-job.json";

/// Where the bytes come from. The verification pipeline is identical for
/// both — that identity is the sideload security answer: a USB stick and a
/// release asset pass the exact same content check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Source {
    /// HTTP(S) release assets under a base URL, e.g.
    /// `https://github.com/cameronzucker/tuxlink/releases/download/v0.106.0`.
    /// Asset names come from [`tuxlink_classify::pins::PinnedModel::asset_name`].
    ///
    /// `custom` marks an operator-typed base (anything but this build's
    /// default). Custom sources fetch under the SSRF-1 egress posture
    /// (`docs/pitfalls/implementation-pitfalls.md`): no redirects, and the
    /// resolved address is gated + pinned at fetch time. The default source
    /// is binary-controlled, so it keeps its https-only redirect chain
    /// (GitHub serves assets via one redirect hop).
    Release {
        base_url: String,
        #[serde(default)]
        custom: bool,
    },
    /// A local directory holding the three files under their plain names
    /// (USB stick, LAN mount, operator-fetched folder).
    Sideload { dir: PathBuf },
}

impl Source {
    /// One-line operator-facing description of the source.
    pub fn describe(&self) -> String {
        match self {
            Source::Release { base_url, .. } => base_url.clone(),
            Source::Sideload { dir } => format!("folder {}", dir.display()),
        }
    }
}

/// Why a job that is not moving is not moving.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureClass {
    /// The network refused or the transfer broke mid-stream. Retried
    /// automatically with backoff while the app runs; resumed on restart.
    Network,
    /// The source itself is wrong: asset missing (404 — e.g. a release that
    /// carries no weights), a served size that cannot match the pin, or a
    /// stream that kept going past it. Not auto-retried — retrying a source
    /// that HAS the wrong thing yields the wrong thing again; the operator
    /// switches source or sideloads.
    Source,
    /// A completed file's sha256 did not match the release pin. HARD failure:
    /// never auto-retried, names the file, and the offending bytes were
    /// removed. Retrying re-fetches from scratch.
    DigestMismatch,
    /// Local filesystem trouble (no space, permissions, disappeared dir).
    Io,
    /// The operator cancelled. The `.part` remains for a later resume.
    Cancelled,
}

impl FailureClass {
    /// Whether the runner may retry this failure without a fresh operator
    /// act. Deliberately only `Network`: a digest mismatch re-downloading in
    /// a loop would hammer a source that is serving wrong bytes, and an IO
    /// failure (full disk) does not heal by itself.
    pub fn auto_retryable(&self) -> bool {
        matches!(self, FailureClass::Network)
    }
}

/// The job's phase. One job exists at a time; `Complete` records stay in
/// place as the "installed" provenance line until a new job replaces them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "phase", rename_all = "snake_case")]
pub enum Phase {
    /// Queued, or backing off after a network failure. `detail` says which
    /// ("starting", "network unreachable — retrying …").
    Waiting { detail: String },
    /// Bytes are flowing for `file`.
    Downloading { file: String },
    /// A file finished streaming and its digest is being finalized, or an
    /// already-present file is being hashed to see whether it can be kept.
    Verifying { file: String },
    /// Every required file is on disk and digest-verified against the pins.
    Complete { at_unix: u64 },
    /// Stopped. See [`FailureClass`] for what may restart it.
    Failed { detail: String, class: FailureClass },
}

impl Phase {
    /// Whether an app boot should put this job back to work without a fresh
    /// operator act. The original start WAS the consent; resuming it is the
    /// same act continuing. Hard failures and completion are terminal.
    pub fn resumable_on_boot(&self) -> bool {
        match self {
            Phase::Waiting { .. } | Phase::Downloading { .. } | Phase::Verifying { .. } => true,
            Phase::Failed { class, .. } => class.auto_retryable(),
            Phase::Complete { .. } => false,
        }
    }
}

/// The persisted job. Everything the runner needs to pick the work back up:
/// the model, the source, the phase, and which files have already been
/// verified-and-installed in THIS job.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobRecord {
    /// Record-format version for forward compatibility.
    pub version: u32,
    pub model_id: String,
    /// App version whose pins vouched for `files_done`. A job resumed by a
    /// DIFFERENT app version must not honor the list: the pins may have
    /// changed under the same file names, and `files_done` is a skip-list
    /// around re-verification. See [`JobRecord::rebase_onto_release`].
    #[serde(default)]
    pub release: String,
    pub source: Source,
    #[serde(flatten)]
    pub phase: Phase,
    /// Files fully verified and renamed into place by this job, in order.
    #[serde(default)]
    pub files_done: Vec<String>,
    pub started_unix: u64,
    pub updated_unix: u64,
}

pub const RECORD_VERSION: u32 = 1;

/// The app version whose compiled-in pins this build enforces.
pub const CURRENT_RELEASE: &str = env!("CARGO_PKG_VERSION");

impl JobRecord {
    pub fn new(model_id: &str, source: Source, now_unix: u64) -> Self {
        JobRecord {
            version: RECORD_VERSION,
            model_id: model_id.to_string(),
            release: CURRENT_RELEASE.to_string(),
            source,
            phase: Phase::Waiting {
                detail: "starting".to_string(),
            },
            files_done: Vec::new(),
            started_unix: now_unix,
            updated_unix: now_unix,
        }
    }

    /// Stamp a resumed record as continued by THIS build. `files_done` is
    /// display-only — the pipeline re-verifies every already-present final
    /// against the CURRENT pins on each pass (the Codex round demonstrated a
    /// skip-list can outlive both the release and the rename durability that
    /// justified it) — so crossing releases only needs the stamp corrected.
    pub fn rebase_onto_release(&mut self) {
        if self.release != CURRENT_RELEASE {
            self.files_done.clear();
            self.release = CURRENT_RELEASE.to_string();
        }
    }
}

/// Load the record from `models_root`, tolerating absence. A present-but-
/// unreadable record is reported as `Err` rather than silently treated as "no
/// job": overwriting a record we could not read would erase the one account
/// of what a previous session was doing.
pub fn load(models_root: &Path) -> Result<Option<JobRecord>, String> {
    let path = models_root.join(JOB_FILE);
    let raw = match std::fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("read {}: {e}", path.display())),
    };
    let record: JobRecord =
        serde_json::from_str(&raw).map_err(|e| format!("parse {}: {e}", path.display()))?;
    Ok(Some(record))
}

/// Persist the record atomically (tmp + rename in the same directory), so a
/// crash mid-write can never leave a half-record — the previous record or the
/// new one, nothing in between.
pub fn store(models_root: &Path, record: &JobRecord) -> Result<(), String> {
    std::fs::create_dir_all(models_root)
        .map_err(|e| format!("create {}: {e}", models_root.display()))?;
    let path = models_root.join(JOB_FILE);
    let tmp = models_root.join(format!("{JOB_FILE}.tmp"));
    let body = serde_json::to_string_pretty(record).map_err(|e| format!("serialize job: {e}"))?;
    std::fs::write(&tmp, body).map_err(|e| format!("write {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("rename {}: {e}", tmp.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static N: AtomicUsize = AtomicUsize::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "tuxlink-weights-job-{}-{tag}-{n}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn round_trips_through_disk() {
        let root = scratch("roundtrip");
        let mut rec = JobRecord::new(
            "bge-small-en-v1.5",
            Source::Release {
                base_url: "https://example.invalid/dl/v1".into(),
                custom: false,
            },
            1_700_000_000,
        );
        rec.phase = Phase::Downloading {
            file: "model.safetensors".into(),
        };
        rec.files_done = vec!["config.json".into()];
        store(&root, &rec).unwrap();
        assert_eq!(load(&root).unwrap(), Some(rec));
    }

    #[test]
    fn absent_record_is_none_but_garbage_is_an_error() {
        let root = scratch("absent");
        assert_eq!(load(&root).unwrap(), None);
        std::fs::write(root.join(JOB_FILE), "{not json").unwrap();
        assert!(load(&root).is_err());
    }

    #[test]
    fn boot_resume_covers_exactly_the_in_flight_and_network_phases() {
        let waiting = Phase::Waiting { detail: "x".into() };
        let downloading = Phase::Downloading { file: "f".into() };
        let verifying = Phase::Verifying { file: "f".into() };
        let complete = Phase::Complete { at_unix: 1 };
        let net = Phase::Failed {
            detail: "x".into(),
            class: FailureClass::Network,
        };
        let digest = Phase::Failed {
            detail: "x".into(),
            class: FailureClass::DigestMismatch,
        };
        let io = Phase::Failed {
            detail: "x".into(),
            class: FailureClass::Io,
        };
        let cancelled = Phase::Failed {
            detail: "x".into(),
            class: FailureClass::Cancelled,
        };

        assert!(waiting.resumable_on_boot());
        assert!(downloading.resumable_on_boot());
        assert!(verifying.resumable_on_boot());
        assert!(net.resumable_on_boot());
        assert!(!complete.resumable_on_boot());
        assert!(!digest.resumable_on_boot());
        assert!(!io.resumable_on_boot());
        assert!(!cancelled.resumable_on_boot());
    }

    #[test]
    fn only_network_failures_auto_retry() {
        assert!(FailureClass::Network.auto_retryable());
        assert!(!FailureClass::Source.auto_retryable());
        assert!(!FailureClass::DigestMismatch.auto_retryable());
        assert!(!FailureClass::Io.auto_retryable());
        assert!(!FailureClass::Cancelled.auto_retryable());
    }

    #[test]
    fn a_record_from_another_release_forfeits_its_skip_list() {
        let mut rec = JobRecord::new(
            "m",
            Source::Release {
                base_url: "https://x".into(),
                custom: false,
            },
            1,
        );
        rec.files_done = vec!["config.json".into()];

        // Same release: the list survives.
        rec.rebase_onto_release();
        assert_eq!(rec.files_done, vec!["config.json"]);

        // Different release (an upgrade happened mid-job): the list is a
        // skip-around-verification and must be discarded.
        rec.release = "0.0.1-other".into();
        rec.rebase_onto_release();
        assert!(rec.files_done.is_empty());
        assert_eq!(rec.release, CURRENT_RELEASE);

        // A pre-`release`-field record deserializes to an empty string and is
        // treated as another release.
        let legacy: JobRecord = serde_json::from_str(
            &serde_json::to_string(&rec).unwrap().replace(
                &format!("\"release\":\"{CURRENT_RELEASE}\""),
                "\"release\":\"\"",
            ),
        )
        .unwrap();
        let mut legacy = legacy;
        legacy.files_done = vec!["config.json".into()];
        legacy.rebase_onto_release();
        assert!(legacy.files_done.is_empty());
    }

    #[test]
    fn wire_shape_is_flat_and_snake_case() {
        // The record is read by humans debugging a stuck field install; the
        // phase flattens into the top level rather than nesting.
        let rec = JobRecord::new(
            "bge-small-en-v1.5",
            Source::Sideload {
                dir: PathBuf::from("/media/usb0/models"),
            },
            7,
        );
        let v: serde_json::Value = serde_json::to_value(&rec).unwrap();
        assert_eq!(v["phase"], "waiting");
        assert_eq!(v["source"]["kind"], "sideload");
        assert_eq!(v["model_id"], "bge-small-en-v1.5");
    }
}
