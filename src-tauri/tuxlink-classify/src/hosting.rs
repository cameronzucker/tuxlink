//! Where T1 weights live on disk, and what happens when they do not.
//!
//! [`CandleBert::load`](crate::candle_bert::CandleBert::load) takes a model
//! directory and trusts it. Nothing decided WHICH directory, so a missing or
//! half-copied file surfaced as an opaque error from inside candle
//! (`No such file or directory (os error 2)` with no indication of which file,
//! which model, or where we looked). This module is the resolution layer:
//! given a model id, find a usable directory or say precisely why there isn't
//! one.
//!
//! Three properties this deliberately has, each load-bearing:
//!
//! 1. **Pure `std`, no network.** ADR 0030's deployment matrix says Tuxlink
//!    spawns nothing and fetches nothing the operator did not ask for. The
//!    crate carries no HTTP client (see [`crate::backend`]), so "we never
//!    silently download weights" is provable by dependency absence rather
//!    than asserted in prose. Keep it that way.
//! 2. **Available without the ML tier.** This module is NOT behind
//!    `t1-candle`. A `--no-default-features` build still needs to REPORT that
//!    T1 is unavailable — a status surface that vanishes when the feature is
//!    off would make the degraded build silently claim nothing is wrong.
//! 3. **Never silently degrades.** A half-written model directory does not
//!    fall through to "absent"; it reports [`ModelStatus::Incomplete`] naming
//!    the missing or wrong-sized files, and a shadowed bad root is still
//!    disclosed in [`Located::shadowed`] even when a later root works.
//!
//! Integrity: an optional `manifest.json` beside the weights declares each
//! file's exact byte length; when present, sizes are verified on every locate.
//! That catches the real-world failure — a truncated copy, an interrupted
//! transfer, a full disk — for free. It is explicitly NOT tamper detection;
//! see [`Integrity`].

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Files a HuggingFace-format BERT directory must carry for
/// [`CandleBert::load`](crate::candle_bert::CandleBert::load) to succeed. The
/// loader reads exactly these three.
pub const REQUIRED_FILES: [&str; 3] = ["config.json", "tokenizer.json", "model.safetensors"];

/// Optional integrity/companion file. Absent is normal and fine.
pub const MANIFEST_FILE: &str = "manifest.json";

/// Environment override for the search path (`:`-separated, highest priority).
pub const MODEL_PATH_ENV: &str = "TUXLINK_CLASSIFY_MODEL_PATH";

/// Cap on how many roots we will search. A pathological `:`-separated env
/// value should not turn every classifier status probe into a directory walk
/// of unbounded length. Overflow is DISCLOSED, never silently dropped
/// ([`Located::roots_truncated`] / [`ModelStatus::Absent::roots_truncated`]).
pub const MAX_ROOTS: usize = 16;

/// How thoroughly a located model was checked. Reported rather than assumed so
/// a caller (and an operator reading a status pane) can tell a verified
/// directory from a merely-present one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Integrity {
    /// A `manifest.json` was present and every required file matched its
    /// declared byte length. Catches truncation, interrupted transfer, and
    /// partial copies.
    ///
    /// NOT tamper detection: byte lengths are trivially forgeable, and this
    /// module intentionally pulls in no hashing dependency. Detecting a
    /// deliberately-substituted model needs a digest, which in turn needs a
    /// trusted source for the expected digest — a distribution question, not
    /// a loader question.
    SizeVerified,
    /// No manifest was present. Files exist and are non-empty; nothing more
    /// is claimed.
    PresenceOnly,
}

/// A model directory that is ready to hand to the loader.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Located {
    /// Directory containing the required files.
    pub dir: PathBuf,
    /// Which search root it was found under.
    pub root: PathBuf,
    pub integrity: Integrity,
    /// Roots that held a broken candidate for this model and were passed over.
    /// A working later root does NOT excuse an earlier broken one: a
    /// half-finished copy in the user data dir shadowed by a good system
    /// install is a real problem the operator should see.
    pub shadowed: Vec<Rejected>,
    /// True when the configured search path exceeded [`MAX_ROOTS`].
    pub roots_truncated: bool,
}

/// A search root that could not supply this model, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rejected {
    pub dir: PathBuf,
    pub reason: Reason,
}

/// Characters permitted in a model id. Deliberately narrow: model ids name a
/// SUBDIRECTORY, so anything that can traverse (`/`, `\`, `..`) or resolve
/// oddly (leading `.`, NUL, whitespace) is refused rather than sanitized.
/// Rejecting is safe here because the id set is small, known, and
/// code/config-owned — silently rewriting an id would instead load weights
/// the caller did not ask for.
fn model_id_is_safe(model_id: &str) -> bool {
    !model_id.is_empty()
        && model_id.len() <= 128
        && model_id != "."
        && model_id != ".."
        && !model_id.starts_with('.')
        && model_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

/// Why one candidate directory was unusable. Ordered roughly by how alarming
/// it is: a directory that simply isn't there is routine, a size mismatch is
/// not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reason {
    /// The candidate directory does not exist. The ordinary case for roots
    /// that simply do not host this model.
    NoDirectory,
    /// Directory exists but required files are missing.
    MissingFiles(Vec<String>),
    /// A required file exists but is zero bytes — the classic signature of an
    /// interrupted write.
    EmptyFiles(Vec<String>),
    /// `manifest.json` declared a byte length that the file on disk does not
    /// match. Names actual vs declared so the report is actionable.
    SizeMismatch(Vec<SizeMismatch>),
    /// `manifest.json` was present but unreadable or malformed. Treated as
    /// unusable rather than ignored: a corrupt manifest beside weights means
    /// something wrote that directory badly, and silently downgrading to
    /// "presence only" would hide it.
    BadManifest(String),
    /// The model id itself is not a safe single path segment. Reported once,
    /// against no particular root, because nothing was searched.
    UnsafeModelId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SizeMismatch {
    pub file: String,
    pub declared: u64,
    pub actual: u64,
}

/// The outcome of looking for one model across the search path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelStatus {
    Ready(Located),
    /// Nothing usable. `rejected` carries one entry per root that was
    /// examined, so the operator-facing message can say where we looked
    /// instead of a bare "not found".
    Absent {
        model_id: String,
        rejected: Vec<Rejected>,
        roots_truncated: bool,
    },
}

impl ModelStatus {
    pub fn is_ready(&self) -> bool {
        matches!(self, ModelStatus::Ready(_))
    }

    /// Directory to load from, if any.
    pub fn dir(&self) -> Option<&Path> {
        match self {
            ModelStatus::Ready(l) => Some(&l.dir),
            ModelStatus::Absent { .. } => None,
        }
    }

    /// One-line operator-facing summary. Deliberately names paths: "model not
    /// found" without a search path is the unhelpful message this module
    /// exists to replace.
    pub fn summary(&self) -> String {
        match self {
            ModelStatus::Ready(l) => {
                let integrity = match l.integrity {
                    Integrity::SizeVerified => "size-verified against manifest.json",
                    Integrity::PresenceOnly => "present (no manifest.json to verify against)",
                };
                let mut s = format!("ready at {} — {}", l.dir.display(), integrity);
                if !l.shadowed.is_empty() {
                    s.push_str(&format!(
                        "; WARNING {} earlier location(s) held an unusable copy: {}",
                        l.shadowed.len(),
                        describe_rejections(&l.shadowed)
                    ));
                }
                s
            }
            ModelStatus::Absent {
                model_id,
                rejected,
                roots_truncated,
            } => {
                let mut s = if rejected.is_empty() {
                    format!("'{model_id}' not found — no search roots are configured")
                } else {
                    format!(
                        "'{model_id}' not found in {} location(s): {}",
                        rejected.len(),
                        describe_rejections(rejected)
                    )
                };
                if *roots_truncated {
                    s.push_str(&format!(
                        " (search path truncated at {MAX_ROOTS} roots; later entries were not examined)"
                    ));
                }
                s
            }
        }
    }
}

fn describe_rejections(rejected: &[Rejected]) -> String {
    rejected
        .iter()
        .map(|r| {
            let why = match &r.reason {
                Reason::NoDirectory => "no such directory".to_string(),
                Reason::MissingFiles(f) => format!("missing {}", f.join(", ")),
                Reason::EmptyFiles(f) => format!("empty {}", f.join(", ")),
                Reason::SizeMismatch(m) => format!(
                    "size mismatch {}",
                    m.iter()
                        .map(|x| format!(
                            "{} (declared {} bytes, found {})",
                            x.file, x.declared, x.actual
                        ))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                Reason::BadManifest(e) => format!("unreadable manifest.json: {e}"),
                Reason::UnsafeModelId => {
                    "model id is not a safe path segment; nothing was searched".to_string()
                }
            };
            format!("{} [{}]", r.dir.display(), why)
        })
        .collect::<Vec<_>>()
        .join("; ")
}

/// Resolves model ids to directories across an ordered search path.
///
/// Layout convention: each root contains one subdirectory per model id, e.g.
/// `<root>/bge-small-en-v1.5/{config.json,tokenizer.json,model.safetensors}`.
#[derive(Debug, Clone)]
pub struct ModelLocator {
    roots: Vec<PathBuf>,
    roots_truncated: bool,
}

impl ModelLocator {
    /// Build from an explicit ordered root list. Earlier roots win.
    pub fn new(roots: impl IntoIterator<Item = PathBuf>) -> Self {
        let all: Vec<PathBuf> = roots.into_iter().collect();
        let roots_truncated = all.len() > MAX_ROOTS;
        Self {
            roots: all.into_iter().take(MAX_ROOTS).collect(),
            roots_truncated,
        }
    }

    /// Build the default search path from the process environment.
    ///
    /// Reads env only — no filesystem probing, no network, no directory
    /// creation. Precedence, highest first:
    ///
    /// 1. `TUXLINK_CLASSIFY_MODEL_PATH` (`:`-separated) — the operator override.
    /// 2. `$XDG_DATA_HOME/tuxlink/models`, else `$HOME/.local/share/tuxlink/models`.
    /// 3. `/usr/share/tuxlink/models` — where a distribution package would put them.
    pub fn from_env() -> Self {
        Self::new(default_roots(
            std::env::var(MODEL_PATH_ENV).ok().as_deref(),
            std::env::var("XDG_DATA_HOME").ok().as_deref(),
            std::env::var("HOME").ok().as_deref(),
        ))
    }

    pub fn roots(&self) -> &[PathBuf] {
        &self.roots
    }

    /// Look for `model_id` across the search path. First root with a fully
    /// usable copy wins; broken earlier candidates are reported via
    /// [`Located::shadowed`] rather than discarded.
    pub fn locate(&self, model_id: &str) -> ModelStatus {
        // A model id names one subdirectory. `root.join("../../etc")` would
        // escape the search root entirely, so the id is validated BEFORE any
        // path is built rather than trusted because "ids come from config".
        if !model_id_is_safe(model_id) {
            return ModelStatus::Absent {
                model_id: model_id.to_string(),
                rejected: vec![Rejected {
                    dir: PathBuf::new(),
                    reason: Reason::UnsafeModelId,
                }],
                roots_truncated: self.roots_truncated,
            };
        }

        let mut rejected: Vec<Rejected> = Vec::new();

        for root in &self.roots {
            let dir = root.join(model_id);
            match inspect(&dir) {
                Ok(integrity) => {
                    return ModelStatus::Ready(Located {
                        dir,
                        root: root.clone(),
                        integrity,
                        shadowed: rejected,
                        roots_truncated: self.roots_truncated,
                    });
                }
                Err(reason) => rejected.push(Rejected { dir, reason }),
            }
        }

        ModelStatus::Absent {
            model_id: model_id.to_string(),
            rejected,
            roots_truncated: self.roots_truncated,
        }
    }
}

/// Split the env inputs into an ordered root list. Pure function so the
/// precedence rules are testable without mutating process environment
/// (`set_var` is unsafe and racy across parallel test threads).
pub fn default_roots(
    override_path: Option<&str>,
    xdg_data_home: Option<&str>,
    home: Option<&str>,
) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();

    if let Some(raw) = override_path {
        for part in raw.split(':') {
            let part = part.trim();
            if !part.is_empty() {
                roots.push(PathBuf::from(part));
            }
        }
    }

    match xdg_data_home.map(str::trim).filter(|s| !s.is_empty()) {
        Some(xdg) => roots.push(Path::new(xdg).join("tuxlink").join("models")),
        None => {
            if let Some(h) = home.map(str::trim).filter(|s| !s.is_empty()) {
                roots.push(
                    Path::new(h)
                        .join(".local")
                        .join("share")
                        .join("tuxlink")
                        .join("models"),
                );
            }
        }
    }

    roots.push(PathBuf::from("/usr/share/tuxlink/models"));
    roots
}

/// Check one candidate directory. `Ok` means every required file is present,
/// non-empty, and (when a manifest declares a length) exactly that length.
fn inspect(dir: &Path) -> Result<Integrity, Reason> {
    if !dir.is_dir() {
        return Err(Reason::NoDirectory);
    }

    let mut missing = Vec::new();
    let mut empty = Vec::new();
    let mut sizes: BTreeMap<&str, u64> = BTreeMap::new();

    for name in REQUIRED_FILES {
        match std::fs::metadata(dir.join(name)) {
            Ok(m) if !m.is_file() => missing.push(name.to_string()),
            Ok(m) if m.len() == 0 => {
                empty.push(name.to_string());
            }
            Ok(m) => {
                sizes.insert(name, m.len());
            }
            Err(_) => missing.push(name.to_string()),
        }
    }

    if !missing.is_empty() {
        return Err(Reason::MissingFiles(missing));
    }
    if !empty.is_empty() {
        return Err(Reason::EmptyFiles(empty));
    }

    match read_manifest_sizes(dir)? {
        None => Ok(Integrity::PresenceOnly),
        Some(declared) => {
            let mismatches: Vec<SizeMismatch> = REQUIRED_FILES
                .iter()
                .filter_map(|name| {
                    let want = declared.get(*name)?;
                    let got = *sizes.get(*name)?;
                    (*want != got).then_some(SizeMismatch {
                        file: (*name).to_string(),
                        declared: *want,
                        actual: got,
                    })
                })
                .collect();
            if mismatches.is_empty() {
                Ok(Integrity::SizeVerified)
            } else {
                Err(Reason::SizeMismatch(mismatches))
            }
        }
    }
}

/// Parse the optional `manifest.json`, returning declared byte lengths.
///
/// Shape (extra keys ignored so the file can carry provenance/licence notes):
/// `{"files": {"model.safetensors": {"bytes": 133466304}, ...}}`
///
/// Absent → `Ok(None)`. Present-but-broken → `Err`, never a silent downgrade
/// to presence-only checking.
fn read_manifest_sizes(dir: &Path) -> Result<Option<BTreeMap<String, u64>>, Reason> {
    let path = dir.join(MANIFEST_FILE);
    let raw = match std::fs::read_to_string(&path) {
        Ok(r) => r,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(Reason::BadManifest(e.to_string())),
    };

    let doc: serde_json::Value =
        serde_json::from_str(&raw).map_err(|e| Reason::BadManifest(e.to_string()))?;

    let files = doc
        .get("files")
        .and_then(|f| f.as_object())
        .ok_or_else(|| Reason::BadManifest("missing object field 'files'".to_string()))?;

    let mut out = BTreeMap::new();
    for (name, entry) in files {
        if let Some(bytes) = entry.get("bytes").and_then(|b| b.as_u64()) {
            out.insert(name.clone(), bytes);
        }
    }
    Ok(Some(out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Scratch directory under the OS temp dir. Avoids a dev-dependency on
    /// tempfile for a handful of tests; each gets a unique name from a
    /// counter plus the test process id.
    fn scratch(tag: &str) -> PathBuf {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static N: AtomicUsize = AtomicUsize::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "tuxlink-hosting-{}-{}-{}",
            std::process::id(),
            tag,
            n
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_model(root: &Path, model_id: &str) -> PathBuf {
        let dir = root.join(model_id);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("config.json"), b"{}").unwrap();
        fs::write(dir.join("tokenizer.json"), b"{}").unwrap();
        fs::write(dir.join("model.safetensors"), b"weights-go-here").unwrap();
        dir
    }

    #[test]
    fn locates_a_complete_model_and_reports_presence_only() {
        let root = scratch("complete");
        let dir = write_model(&root, "bge-small-en-v1.5");

        let status = ModelLocator::new([root.clone()]).locate("bge-small-en-v1.5");

        match &status {
            ModelStatus::Ready(l) => {
                assert_eq!(l.dir, dir);
                assert_eq!(l.root, root);
                assert_eq!(l.integrity, Integrity::PresenceOnly);
                assert!(l.shadowed.is_empty());
            }
            other => panic!("expected Ready, got {other:?}"),
        }
        assert!(status.summary().contains("no manifest.json"));
    }

    #[test]
    fn absent_model_names_every_place_it_looked() {
        let a = scratch("absent-a");
        let b = scratch("absent-b");

        let status = ModelLocator::new([a.clone(), b.clone()]).locate("bge-small-en-v1.5");

        match &status {
            ModelStatus::Absent { rejected, .. } => {
                assert_eq!(rejected.len(), 2);
                assert!(rejected.iter().all(|r| r.reason == Reason::NoDirectory));
            }
            other => panic!("expected Absent, got {other:?}"),
        }
        // The whole point: the message is actionable, not "not found".
        let s = status.summary();
        assert!(s.contains(&a.display().to_string()));
        assert!(s.contains(&b.display().to_string()));
    }

    #[test]
    fn missing_file_reports_which_file_not_just_absent() {
        let root = scratch("partial");
        let dir = root.join("bge-small-en-v1.5");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("config.json"), b"{}").unwrap();

        let status = ModelLocator::new([root]).locate("bge-small-en-v1.5");

        match &status {
            ModelStatus::Absent { rejected, .. } => match &rejected[0].reason {
                Reason::MissingFiles(f) => {
                    assert!(f.contains(&"tokenizer.json".to_string()));
                    assert!(f.contains(&"model.safetensors".to_string()));
                    assert!(!f.contains(&"config.json".to_string()));
                }
                other => panic!("expected MissingFiles, got {other:?}"),
            },
            other => panic!("expected Absent, got {other:?}"),
        }
    }

    #[test]
    fn zero_byte_weights_are_incomplete_not_ready() {
        // The signature of an interrupted write. Presence alone would call
        // this ready and hand candle a file it will choke on.
        let root = scratch("truncated");
        let dir = root.join("m");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("config.json"), b"{}").unwrap();
        fs::write(dir.join("tokenizer.json"), b"{}").unwrap();
        fs::write(dir.join("model.safetensors"), b"").unwrap();

        let status = ModelLocator::new([root]).locate("m");
        match &status {
            ModelStatus::Absent { rejected, .. } => {
                assert_eq!(
                    rejected[0].reason,
                    Reason::EmptyFiles(vec!["model.safetensors".to_string()])
                );
            }
            other => panic!("expected Absent, got {other:?}"),
        }
    }

    #[test]
    fn manifest_size_match_upgrades_integrity_to_size_verified() {
        let root = scratch("manifest-ok");
        let dir = write_model(&root, "m");
        let len = fs::metadata(dir.join("model.safetensors")).unwrap().len();
        fs::write(
            dir.join(MANIFEST_FILE),
            format!(
                r#"{{"note":"extra keys ignored","files":{{"model.safetensors":{{"bytes":{len}}}}}}}"#
            ),
        )
        .unwrap();

        let status = ModelLocator::new([root]).locate("m");
        match &status {
            ModelStatus::Ready(l) => assert_eq!(l.integrity, Integrity::SizeVerified),
            other => panic!("expected Ready, got {other:?}"),
        }
        assert!(status.summary().contains("size-verified"));
    }

    #[test]
    fn manifest_size_mismatch_rejects_and_reports_both_numbers() {
        let root = scratch("manifest-bad");
        let dir = write_model(&root, "m");
        fs::write(
            dir.join(MANIFEST_FILE),
            r#"{"files":{"model.safetensors":{"bytes":999999}}}"#,
        )
        .unwrap();

        let status = ModelLocator::new([root]).locate("m");
        match &status {
            ModelStatus::Absent { rejected, .. } => match &rejected[0].reason {
                Reason::SizeMismatch(m) => {
                    assert_eq!(m[0].file, "model.safetensors");
                    assert_eq!(m[0].declared, 999_999);
                    assert_eq!(m[0].actual, 15);
                }
                other => panic!("expected SizeMismatch, got {other:?}"),
            },
            other => panic!("expected Absent, got {other:?}"),
        }
    }

    #[test]
    fn corrupt_manifest_is_an_error_not_a_silent_downgrade() {
        let root = scratch("manifest-corrupt");
        let dir = write_model(&root, "m");
        fs::write(dir.join(MANIFEST_FILE), b"{not json").unwrap();

        let status = ModelLocator::new([root]).locate("m");
        match &status {
            ModelStatus::Absent { rejected, .. } => {
                assert!(matches!(rejected[0].reason, Reason::BadManifest(_)));
            }
            other => panic!("expected Absent, got {other:?}"),
        }
    }

    #[test]
    fn a_working_later_root_still_discloses_the_broken_earlier_one() {
        // A half-finished copy in the user dir shadowed by a good system
        // install must not disappear from the report.
        let bad = scratch("shadow-bad");
        let good = scratch("shadow-good");
        fs::create_dir_all(bad.join("m")).unwrap();
        fs::write(bad.join("m").join("config.json"), b"{}").unwrap();
        write_model(&good, "m");

        let status = ModelLocator::new([bad.clone(), good.clone()]).locate("m");
        match &status {
            ModelStatus::Ready(l) => {
                assert_eq!(l.root, good);
                assert_eq!(l.shadowed.len(), 1);
                assert_eq!(l.shadowed[0].dir, bad.join("m"));
            }
            other => panic!("expected Ready, got {other:?}"),
        }
        assert!(status.summary().contains("WARNING"));
    }

    #[test]
    fn root_list_is_capped_and_the_truncation_is_disclosed() {
        let roots: Vec<PathBuf> = (0..MAX_ROOTS + 5)
            .map(|i| PathBuf::from(format!("/nonexistent/root{i}")))
            .collect();
        let loc = ModelLocator::new(roots);
        assert_eq!(loc.roots().len(), MAX_ROOTS);

        let status = loc.locate("m");
        match &status {
            ModelStatus::Absent {
                roots_truncated, ..
            } => assert!(*roots_truncated),
            other => panic!("expected Absent, got {other:?}"),
        }
        assert!(status.summary().contains("truncated"));
    }

    #[test]
    fn traversing_model_ids_are_refused_before_any_path_is_built() {
        // `root.join("../../etc")` escapes the search root. Validate the id
        // rather than trusting it because "ids come from config".
        let root = scratch("traversal");
        write_model(&root, "m");
        let loc = ModelLocator::new([root.clone()]);

        for bad in [
            "../../etc",
            "..",
            ".",
            "a/b",
            "a\\b",
            "/absolute",
            ".hidden",
            "",
            "has space",
            "nul\0byte",
        ] {
            let status = loc.locate(bad);
            match &status {
                ModelStatus::Absent { rejected, .. } => assert_eq!(
                    rejected[0].reason,
                    Reason::UnsafeModelId,
                    "expected {bad:?} to be refused as an unsafe id"
                ),
                other => panic!("expected Absent for {bad:?}, got {other:?}"),
            }
        }

        // The ordinary case still resolves, including dotted version ids.
        assert!(loc.locate("m").is_ready());
        assert!(model_id_is_safe("bge-small-en-v1.5"));
        assert!(model_id_is_safe("all_MiniLM-L6-v2"));
        // Over-long ids are refused too (128 is well past any real model id).
        assert!(!model_id_is_safe(&"a".repeat(129)));
    }

    /// End-to-end against REAL weights: the locator must hand candle a
    /// directory candle can actually load. A resolver whose output the loader
    /// rejects is worse than no resolver, and unit tests over fake files
    /// cannot catch that — they only prove the locator agrees with itself.
    ///
    /// Ignored by default because it needs ~134MB of weights on disk; CI has
    /// none. Run where a model is materialized:
    ///
    /// ```text
    /// TUXLINK_CLASSIFY_MODEL_PATH=$HOME/classify-models \
    ///   cargo test --features t1-candle -- --ignored locator_output_actually_loads
    /// ```
    #[test]
    #[ignore = "needs real weights on disk; see doc comment for the invocation"]
    #[cfg(feature = "t1-candle")]
    fn locator_output_actually_loads_and_embeds() {
        use crate::backend::{EmbeddingBackend, Pooling};
        use crate::candle_bert::CandleBert;

        let status = ModelLocator::from_env().locate("bge-small-en-v1.5");
        let located = match &status {
            ModelStatus::Ready(l) => l,
            other => panic!("locator found nothing: {}\n{other:?}", status.summary()),
        };

        // bge is a CLS-pooling family; getting this wrong silently costs
        // accuracy while still "working", so the e2e check asserts real
        // vector behaviour rather than just a successful load.
        let bert = CandleBert::load(&located.dir, Pooling::Cls, "bge-small-en-v1.5")
            .expect("candle rejected the directory the locator returned");

        let vecs = bert
            .embed(&[
                "request a weather forecast for Arizona".to_string(),
                "weather bulletin for the southwest".to_string(),
                "replace the antenna feedline connector".to_string(),
            ])
            .expect("embed failed");

        assert_eq!(vecs.len(), 3);
        assert_eq!(vecs[0].len(), 384, "bge-small is a 384-dim model");

        // L2-normalised, per the EmbeddingBackend contract (dot == cosine).
        for v in &vecs {
            let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
            assert!((norm - 1.0).abs() < 1e-3, "vector not unit-length: {norm}");
        }

        let dot = |a: &[f32], b: &[f32]| a.iter().zip(b).map(|(x, y)| x * y).sum::<f32>();
        let related = dot(&vecs[0], &vecs[1]);
        let unrelated = dot(&vecs[0], &vecs[2]);
        assert!(
            related > unrelated,
            "two weather queries ({related:.3}) should be closer than \
             weather vs antenna hardware ({unrelated:.3}) — a loaded-but-wrong \
             model can still produce unit vectors"
        );
    }

    #[test]
    fn env_precedence_override_then_xdg_then_system() {
        let roots = default_roots(Some("/opt/a:/opt/b"), Some("/xdg"), Some("/home/op"));
        assert_eq!(
            roots,
            vec![
                PathBuf::from("/opt/a"),
                PathBuf::from("/opt/b"),
                PathBuf::from("/xdg/tuxlink/models"),
                PathBuf::from("/usr/share/tuxlink/models"),
            ]
        );
    }

    #[test]
    fn env_falls_back_to_home_when_xdg_unset_and_ignores_blank_entries() {
        let roots = default_roots(Some("/opt/a::  :"), None, Some("/home/op"));
        assert_eq!(
            roots,
            vec![
                PathBuf::from("/opt/a"),
                PathBuf::from("/home/op/.local/share/tuxlink/models"),
                PathBuf::from("/usr/share/tuxlink/models"),
            ]
        );
    }

    #[test]
    fn system_root_is_always_last_even_with_no_env_at_all() {
        // A packaged install with no HOME (a service account, a container)
        // must still find distribution-provided weights.
        assert_eq!(
            default_roots(None, None, None),
            vec![PathBuf::from("/usr/share/tuxlink/models")]
        );
    }
}
