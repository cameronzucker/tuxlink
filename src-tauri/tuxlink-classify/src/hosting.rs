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
//! # What this does and does not promise
//!
//! **No network.** Nothing reachable from here opens a socket, spawns a
//! process, or downloads anything; the only I/O is local-filesystem reads plus
//! JSON parsing of a small manifest. (The crate does pull `serde_json` — an
//! earlier version of this comment claimed "pure std", which was false. And
//! dependency absence would not have been proof anyway, since `std` itself
//! contains networking: the source trace is the proof.)
//!
//! **Structural validation, not loadability.** A directory reported
//! [`ModelStatus::Ready`] has been checked file-by-file: required files exist,
//! are non-empty regular files, the two JSON files parse, and the weights
//! carry a well-formed safetensors header whose declared length fits the file.
//! That is much stronger than presence — an earlier version accepted
//! `config.json` containing `{` and weights containing `x` — but it is still
//! not a guarantee that candle will accept the model. Only loading proves
//! that, and by then the caller has a real error to report.
//!
//! **Never silently degrades.** A directory that fails any check is reported
//! as [`ModelStatus::Absent`] carrying a per-root [`Reason`] that names the
//! offending files, and a broken earlier root stays visible in
//! [`Located::shadowed`] even when a later root works — so a half-finished
//! copy in the user data dir cannot hide behind a good system install.
//!
//! **Not a defence against a hostile local filesystem.** Symlinks are
//! followed, and there is an unavoidable window between checking a file and
//! the loader opening it (TOCTOU). An attacker who can write into a search
//! root can defeat every check here. That is accepted: such an attacker can
//! equally replace the application binary. Weight directories should be
//! operator- or package-owned.
//!
//! Integrity: an optional `manifest.json` declares each required file's exact
//! byte length. When present it must declare ALL of them — a partial manifest
//! is rejected rather than silently verifying only what it happens to mention.
//! It is explicitly NOT tamper detection; see [`Integrity`].

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// Files a HuggingFace-format BERT directory must carry for
/// [`CandleBert::load`](crate::candle_bert::CandleBert::load) to succeed. The
/// loader reads exactly these three.
pub const REQUIRED_FILES: [&str; 3] = ["config.json", "tokenizer.json", "model.safetensors"];

/// Optional integrity/companion file. Absent is normal and fine.
pub const MANIFEST_FILE: &str = "manifest.json";

/// Environment override for the search path (`:`-separated, highest priority).
pub const MODEL_PATH_ENV: &str = "TUXLINK_CLASSIFY_MODEL_PATH";

/// Cap on how many roots we will search. Overflow is DISCLOSED, never
/// silently dropped. Applied to the ITERATOR, so an unbounded root iterator
/// cannot hang or exhaust memory before the cap takes effect.
pub const MAX_ROOTS: usize = 16;

/// Largest `manifest.json` we will read. A manifest is a handful of file
/// lengths; anything larger is malformed or hostile, and an uncapped
/// `read_to_string` on an attacker-chosen path is an allocation gun.
pub const MANIFEST_MAX_BYTES: u64 = 64 * 1024;

/// Largest safetensors JSON header we will accept. Real headers for models in
/// this class are a few hundred KB; the cap stops a corrupt 8-byte length
/// prefix from turning into a huge read.
const SAFETENSORS_HEADER_MAX: u64 = 16 * 1024 * 1024;

/// How thoroughly a located model was checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Integrity {
    /// A `manifest.json` declared every required file's byte length and all
    /// matched. Catches truncation, interrupted transfer, and partial copies.
    ///
    /// NOT tamper detection: byte lengths are trivially forgeable, and this
    /// module intentionally pulls in no hashing dependency. Detecting a
    /// deliberately-substituted model needs a digest, which needs a trusted
    /// source for the expected digest — a distribution question, not a loader
    /// question.
    SizeVerified,
    /// No manifest was present. Files exist, are non-empty regular files, and
    /// are structurally well-formed; nothing about their exact bytes is
    /// claimed.
    StructureOnly,
}

/// A model directory that passed every check.
///
/// Fields are private: this type is a claim that validation happened, and a
/// public-field struct can be minted by any caller with `integrity:
/// SizeVerified` and an arbitrary directory, which would make the claim
/// meaningless.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Located {
    dir: PathBuf,
    root: PathBuf,
    model_id: String,
    integrity: Integrity,
    shadowed: Vec<Rejected>,
    roots_truncated: bool,
}

impl Located {
    /// Directory to hand to the loader.
    pub fn dir(&self) -> &Path {
        &self.dir
    }
    /// Which search root supplied it.
    pub fn root(&self) -> &Path {
        &self.root
    }
    /// The model id this directory was resolved FOR.
    ///
    /// Pass this to the loader rather than a separately-configured label.
    /// `CandleBert::load` takes an independent `model_id` that becomes part of
    /// the threshold-calibration key, so nothing otherwise stops MiniLM
    /// weights being labelled `bge-small-en-v1.5` and scored against
    /// bge-calibrated thresholds.
    pub fn model_id(&self) -> &str {
        &self.model_id
    }
    pub fn integrity(&self) -> Integrity {
        self.integrity
    }
    /// Roots that held a broken candidate and were passed over. A working
    /// later root does NOT excuse an earlier broken one.
    pub fn shadowed(&self) -> &[Rejected] {
        &self.shadowed
    }
    pub fn roots_truncated(&self) -> bool {
        self.roots_truncated
    }
}

/// A search root that could not supply this model, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rejected {
    pub dir: PathBuf,
    pub reason: Reason,
}

impl Rejected {
    /// Whether this rejection is worth telling the operator about. A root that
    /// simply does not host this model is routine; a root holding a BROKEN
    /// copy is not.
    pub fn is_alarming(&self) -> bool {
        !matches!(self.reason, Reason::NoDirectory)
    }
}

/// Why one candidate directory was unusable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reason {
    /// The candidate directory does not exist. The ordinary case for roots
    /// that simply do not host this model.
    NoDirectory,
    /// Required files are missing, or exist but are not regular files.
    MissingFiles(Vec<String>),
    /// A required file is zero bytes — the classic signature of an
    /// interrupted write.
    EmptyFiles(Vec<String>),
    /// A required file exists and is non-empty but is not what it claims to
    /// be: unparseable JSON, or weights without a usable safetensors header.
    Malformed(Vec<MalformedFile>),
    /// `manifest.json` declared a byte length the file on disk does not match.
    SizeMismatch(Vec<SizeMismatch>),
    /// `manifest.json` was present but unusable — unreadable, malformed,
    /// oversized, or not declaring every required file. Treated as an error
    /// rather than ignored: something wrote that directory badly, and
    /// downgrading to a weaker check would hide it.
    BadManifest(String),
    /// The model id itself is not a safe single path segment. Reported once,
    /// against no root, because nothing was searched.
    UnsafeModelId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SizeMismatch {
    pub file: String,
    pub declared: u64,
    pub actual: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MalformedFile {
    pub file: String,
    pub problem: String,
}

/// The outcome of looking for one model across the search path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelStatus {
    Ready(Located),
    /// Nothing usable. `rejected` carries one entry per root examined, so the
    /// operator-facing message can say where we looked instead of a bare "not
    /// found".
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

    pub fn located(&self) -> Option<&Located> {
        match self {
            ModelStatus::Ready(l) => Some(l),
            ModelStatus::Absent { .. } => None,
        }
    }

    pub fn dir(&self) -> Option<&Path> {
        self.located().map(Located::dir)
    }

    /// One-line operator-facing summary. Deliberately names paths: "model not
    /// found" without a search path is the unhelpful message this module
    /// exists to replace.
    pub fn summary(&self) -> String {
        let truncation_note = |t: bool| {
            if t {
                format!(
                    " (search path truncated at {MAX_ROOTS} roots; later entries were not examined)"
                )
            } else {
                String::new()
            }
        };

        match self {
            ModelStatus::Ready(l) => {
                let integrity = match l.integrity {
                    Integrity::SizeVerified => "size-verified against manifest.json",
                    Integrity::StructureOnly => {
                        "structurally valid (no manifest.json to verify sizes against)"
                    }
                };
                let mut s = format!("ready at {} — {}", l.dir.display(), integrity);
                // Only genuinely-broken roots are worth a warning; a root that
                // simply does not host this model is routine.
                let alarming: Vec<&Rejected> =
                    l.shadowed.iter().filter(|r| r.is_alarming()).collect();
                if !alarming.is_empty() {
                    s.push_str(&format!(
                        "; WARNING {} earlier location(s) held an unusable copy: {}",
                        alarming.len(),
                        describe_rejections(alarming.into_iter())
                    ));
                }
                s.push_str(&truncation_note(l.roots_truncated));
                s
            }
            ModelStatus::Absent {
                model_id,
                rejected,
                roots_truncated,
            } => {
                let mut s = if rejected.is_empty() {
                    format!(
                        "'{}' not found — no search roots are configured",
                        display_id(model_id)
                    )
                } else {
                    format!(
                        "'{}' not found in {} location(s): {}",
                        display_id(model_id),
                        rejected.len(),
                        describe_rejections(rejected.iter())
                    )
                };
                s.push_str(&truncation_note(*roots_truncated));
                s
            }
        }
    }
}

/// Render a model id for operator-facing output.
///
/// A rejected id is attacker-influenceable in principle and is echoed back, so
/// it is length-capped and stripped of control characters — otherwise a
/// gigabyte id becomes a gigabyte allocation, and an embedded newline lets the
/// id forge additional operator-visible status lines.
fn display_id(model_id: &str) -> String {
    const MAX: usize = 64;
    let cleaned: String = model_id
        .chars()
        .take(MAX)
        .map(|c| if c.is_control() { '\u{fffd}' } else { c })
        .collect();
    if model_id.chars().nth(MAX).is_some() {
        format!("{cleaned}… ({} chars total)", model_id.chars().count())
    } else {
        cleaned
    }
}

fn describe_rejections<'a>(rejected: impl Iterator<Item = &'a Rejected>) -> String {
    rejected
        .map(|r| {
            let why = match &r.reason {
                Reason::NoDirectory => "no such directory".to_string(),
                Reason::MissingFiles(f) => format!("missing {}", f.join(", ")),
                Reason::EmptyFiles(f) => format!("empty {}", f.join(", ")),
                Reason::Malformed(m) => format!(
                    "malformed {}",
                    m.iter()
                        .map(|x| format!("{} ({})", x.file, x.problem))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
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
                Reason::BadManifest(e) => format!("unusable manifest.json: {e}"),
                Reason::UnsafeModelId => {
                    "model id is not a safe path segment; nothing was searched".to_string()
                }
            };
            format!("{} [{}]", r.dir.display(), why)
        })
        .collect::<Vec<_>>()
        .join("; ")
}

/// Characters permitted in a model id. Deliberately narrow: model ids name a
/// SUBDIRECTORY, so anything that can traverse (`/`, `\`, `..`) or resolve
/// oddly (leading `.`, NUL, whitespace) is refused rather than sanitized.
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
    /// Build from an ordered root list. Earlier roots win. Duplicates are
    /// removed (they would otherwise crowd real roots out of the cap), and the
    /// cap is applied to the ITERATOR so an unbounded source cannot hang or
    /// exhaust memory first.
    pub fn new(roots: impl IntoIterator<Item = PathBuf>) -> Self {
        let mut seen = BTreeSet::new();
        // Pull at most MAX_ROOTS + 1 distinct roots: the extra one is how we
        // learn the caller supplied more than we will search.
        let mut kept: Vec<PathBuf> = roots
            .into_iter()
            .filter(|r| seen.insert(r.clone()))
            .take(MAX_ROOTS + 1)
            .collect();
        let roots_truncated = kept.len() > MAX_ROOTS;
        kept.truncate(MAX_ROOTS);
        Self {
            roots: kept,
            roots_truncated,
        }
    }

    /// Build the default search path from the process environment.
    ///
    /// Reads env only — no filesystem probing, no directory creation.
    /// Precedence, highest first:
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
        // escape the search root, so the id is validated BEFORE any path is
        // built rather than trusted because "ids come from config".
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
                        model_id: model_id.to_string(),
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

/// Check one candidate directory.
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
            Ok(m) if m.len() == 0 => empty.push(name.to_string()),
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

    // Non-empty is not well-formed. Without this, `config.json` containing a
    // single `{` and weights containing `x` were reported Ready and the
    // failure surfaced from inside candle — the exact error this module exists
    // to replace.
    let mut malformed = Vec::new();
    for name in ["config.json", "tokenizer.json"] {
        if let Err(problem) = check_json(&dir.join(name)) {
            malformed.push(MalformedFile {
                file: name.to_string(),
                problem,
            });
        }
    }
    if let Err(problem) = check_safetensors(&dir.join("model.safetensors")) {
        malformed.push(MalformedFile {
            file: "model.safetensors".to_string(),
            problem,
        });
    }
    if !malformed.is_empty() {
        return Err(Reason::Malformed(malformed));
    }

    match read_manifest_sizes(dir)? {
        None => Ok(Integrity::StructureOnly),
        Some(declared) => {
            // A manifest that mentions only some required files must NOT yield
            // SizeVerified over the ones it skipped. Partial is malformed.
            let undeclared: Vec<String> = REQUIRED_FILES
                .iter()
                .filter(|n| !declared.contains_key(**n))
                .map(|n| (*n).to_string())
                .collect();
            if !undeclared.is_empty() {
                return Err(Reason::BadManifest(format!(
                    "does not declare byte lengths for {}",
                    undeclared.join(", ")
                )));
            }

            let mismatches: Vec<SizeMismatch> = REQUIRED_FILES
                .iter()
                .filter_map(|name| {
                    let want = *declared.get(*name)?;
                    let got = *sizes.get(*name)?;
                    (want != got).then_some(SizeMismatch {
                        file: (*name).to_string(),
                        declared: want,
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

/// Read a small file, refusing anything that is not a bounded regular file.
/// Guards against a FIFO (blocks forever), a huge file (allocation), and a
/// device (unbounded).
fn read_small_file(path: &Path, max: u64) -> Result<String, String> {
    let meta = std::fs::metadata(path).map_err(|e| e.to_string())?;
    if !meta.is_file() {
        return Err("not a regular file".to_string());
    }
    if meta.len() > max {
        return Err(format!("{} bytes exceeds the {max}-byte limit", meta.len()));
    }
    std::fs::read_to_string(path).map_err(|e| e.to_string())
}

fn check_json(path: &Path) -> Result<(), String> {
    // Tokenizer files legitimately reach a few MB; config.json is tiny. One
    // generous bound covers both without being an allocation gun.
    let raw = read_small_file(path, 64 * 1024 * 1024)?;
    serde_json::from_str::<serde_json::Value>(&raw)
        .map(|_| ())
        .map_err(|e| format!("not valid JSON: {e}"))
}

/// Validate the safetensors container header without reading the tensors.
///
/// Layout: 8-byte little-endian header length N, then N bytes of JSON, then
/// tensor data. Checking that N is sane, fits inside the file, and parses as a
/// JSON object catches truncation and wrong-format files cheaply.
fn check_safetensors(path: &Path) -> Result<(), String> {
    use std::io::Read;

    let meta = std::fs::metadata(path).map_err(|e| e.to_string())?;
    if !meta.is_file() {
        return Err("not a regular file".to_string());
    }
    let file_len = meta.len();
    if file_len < 8 {
        return Err(format!("{file_len} bytes is too short to hold a header"));
    }

    let mut f = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut len_bytes = [0u8; 8];
    f.read_exact(&mut len_bytes).map_err(|e| e.to_string())?;
    let header_len = u64::from_le_bytes(len_bytes);

    if header_len == 0 {
        return Err("declares a zero-length header".to_string());
    }
    if header_len > SAFETENSORS_HEADER_MAX {
        return Err(format!(
            "declares a {header_len}-byte header, above the {SAFETENSORS_HEADER_MAX}-byte limit \
             (file is probably not safetensors)"
        ));
    }
    if header_len + 8 > file_len {
        return Err(format!(
            "declares a {header_len}-byte header but the file is only {file_len} bytes \
             (truncated or not safetensors)"
        ));
    }

    let mut header = vec![0u8; header_len as usize];
    f.read_exact(&mut header).map_err(|e| e.to_string())?;
    match serde_json::from_slice::<serde_json::Value>(&header) {
        Ok(serde_json::Value::Object(_)) => Ok(()),
        Ok(_) => Err("header is not a JSON object".to_string()),
        Err(e) => Err(format!("header is not valid JSON: {e}")),
    }
}

/// Parse the optional `manifest.json`, returning declared byte lengths.
///
/// Shape (extra top-level keys ignored so the file can carry provenance or
/// licence notes): `{"files": {"model.safetensors": {"bytes": 133466304}}}`
///
/// Absent → `Ok(None)`. Present-but-broken → `Err`, never a silent downgrade.
/// A `bytes` value that is missing, negative, fractional, a string, null, or
/// beyond `u64` is an ERROR rather than a skipped entry — silently ignoring it
/// is how `{"files":{}}` used to produce a "size-verified" verdict.
fn read_manifest_sizes(dir: &Path) -> Result<Option<BTreeMap<String, u64>>, Reason> {
    let path = dir.join(MANIFEST_FILE);

    // Distinguish "no manifest" from "a manifest we cannot read". A dangling
    // symlink reports NotFound from metadata(), so check the link itself
    // before concluding the file is simply absent.
    match std::fs::symlink_metadata(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(Reason::BadManifest(e.to_string())),
        Ok(_) => {}
    }

    let raw = read_small_file(&path, MANIFEST_MAX_BYTES).map_err(Reason::BadManifest)?;

    let doc: serde_json::Value =
        serde_json::from_str(&raw).map_err(|e| Reason::BadManifest(e.to_string()))?;

    let files = doc
        .get("files")
        .and_then(|f| f.as_object())
        .ok_or_else(|| Reason::BadManifest("missing object field 'files'".to_string()))?;

    let mut out = BTreeMap::new();
    for (name, entry) in files {
        // Only required files matter; extra entries are ignored rather than
        // treated as an error, so a manifest may describe optional companions.
        if !REQUIRED_FILES.contains(&name.as_str()) {
            continue;
        }
        let bytes = entry.get("bytes").ok_or_else(|| {
            Reason::BadManifest(format!("entry '{name}' has no 'bytes' field"))
        })?;
        let n = bytes.as_u64().ok_or_else(|| {
            Reason::BadManifest(format!(
                "entry '{name}' has a 'bytes' value that is not a non-negative integer: {bytes}"
            ))
        })?;
        out.insert(name.clone(), n);
    }
    Ok(Some(out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

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

    /// Minimal but STRUCTURALLY VALID safetensors: 8-byte LE header length
    /// followed by that many bytes of JSON object, then a byte of "tensor
    /// data". The previous helper wrote `weights-go-here` and asserted Ready,
    /// which is precisely the weakness the review found — a test fixture that
    /// blesses invalid input.
    fn safetensors_bytes() -> Vec<u8> {
        let header = br#"{"__metadata__":{}}"#;
        let mut v = (header.len() as u64).to_le_bytes().to_vec();
        v.extend_from_slice(header);
        v.push(0);
        v
    }

    fn write_model(root: &Path, model_id: &str) -> PathBuf {
        let dir = root.join(model_id);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("config.json"), b"{}").unwrap();
        fs::write(dir.join("tokenizer.json"), b"{}").unwrap();
        fs::write(dir.join("model.safetensors"), safetensors_bytes()).unwrap();
        dir
    }

    #[test]
    fn locates_a_complete_model_and_reports_structure_only() {
        let root = scratch("complete");
        let dir = write_model(&root, "bge-small-en-v1.5");

        let status = ModelLocator::new([root.clone()]).locate("bge-small-en-v1.5");

        let l = status.located().expect("expected Ready");
        assert_eq!(l.dir(), dir);
        assert_eq!(l.root(), root);
        assert_eq!(l.integrity(), Integrity::StructureOnly);
        assert_eq!(l.model_id(), "bge-small-en-v1.5");
        assert!(l.shadowed().is_empty());
        assert!(status.summary().contains("no manifest.json"));
    }

    #[test]
    fn nonempty_garbage_is_not_ready() {
        // The review's concrete case: presence + non-empty said Ready, and the
        // failure then surfaced from inside candle.
        let root = scratch("garbage");
        let dir = root.join("m");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("config.json"), b"{").unwrap();
        fs::write(dir.join("tokenizer.json"), b"{").unwrap();
        fs::write(dir.join("model.safetensors"), b"x").unwrap();

        let status = ModelLocator::new([root]).locate("m");
        match &status {
            ModelStatus::Absent { rejected, .. } => match &rejected[0].reason {
                Reason::Malformed(m) => {
                    let files: Vec<&str> = m.iter().map(|x| x.file.as_str()).collect();
                    assert!(files.contains(&"config.json"));
                    assert!(files.contains(&"tokenizer.json"));
                    assert!(files.contains(&"model.safetensors"));
                }
                other => panic!("expected Malformed, got {other:?}"),
            },
            other => panic!("expected Absent, got {other:?}"),
        }
    }

    #[test]
    fn truncated_safetensors_is_rejected() {
        let root = scratch("truncated-st");
        let dir = write_model(&root, "m");
        // Header claims far more bytes than the file holds.
        let mut bad = 4096u64.to_le_bytes().to_vec();
        bad.extend_from_slice(b"{}");
        fs::write(dir.join("model.safetensors"), bad).unwrap();

        let status = ModelLocator::new([root]).locate("m");
        match &status {
            ModelStatus::Absent { rejected, .. } => match &rejected[0].reason {
                Reason::Malformed(m) => assert!(m[0].problem.contains("truncated")),
                other => panic!("expected Malformed, got {other:?}"),
            },
            other => panic!("expected Absent, got {other:?}"),
        }
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
    fn zero_byte_weights_are_not_ready() {
        let root = scratch("empty");
        let dir = root.join("m");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("config.json"), b"{}").unwrap();
        fs::write(dir.join("tokenizer.json"), b"{}").unwrap();
        fs::write(dir.join("model.safetensors"), b"").unwrap();

        let status = ModelLocator::new([root]).locate("m");
        match &status {
            ModelStatus::Absent { rejected, .. } => assert_eq!(
                rejected[0].reason,
                Reason::EmptyFiles(vec!["model.safetensors".to_string()])
            ),
            other => panic!("expected Absent, got {other:?}"),
        }
    }

    fn write_manifest(dir: &Path, body: &str) {
        fs::write(dir.join(MANIFEST_FILE), body).unwrap();
    }

    #[test]
    fn manifest_declaring_every_required_file_upgrades_to_size_verified() {
        let root = scratch("manifest-ok");
        let dir = write_model(&root, "m");
        let len = |n: &str| fs::metadata(dir.join(n)).unwrap().len();
        write_manifest(
            &dir,
            &format!(
                r#"{{"note":"extra keys ignored","files":{{
                   "config.json":{{"bytes":{}}},
                   "tokenizer.json":{{"bytes":{}}},
                   "model.safetensors":{{"bytes":{}}}}}}}"#,
                len("config.json"),
                len("tokenizer.json"),
                len("model.safetensors")
            ),
        );

        let status = ModelLocator::new([root]).locate("m");
        assert_eq!(
            status.located().expect("Ready").integrity(),
            Integrity::SizeVerified
        );
        assert!(status.summary().contains("size-verified"));
    }

    #[test]
    fn partial_or_junk_manifests_never_yield_size_verified() {
        // Every one of these previously produced SizeVerified over bytes that
        // were never checked, because an undeclared file was silently skipped
        // and a non-integer `bytes` vanished through as_u64.
        for body in [
            r#"{"files":{}}"#,
            r#"{"files":{"evil.bin":{"bytes":1}}}"#,
            r#"{"files":{"config.json":{"bytes":2}}}"#,
            r#"{"files":{"config.json":{"bytes":-1},"tokenizer.json":{"bytes":2},"model.safetensors":{"bytes":3}}}"#,
            r#"{"files":{"config.json":{"bytes":"2"},"tokenizer.json":{"bytes":2},"model.safetensors":{"bytes":3}}}"#,
            r#"{"files":{"config.json":{"bytes":1.5},"tokenizer.json":{"bytes":2},"model.safetensors":{"bytes":3}}}"#,
            r#"{"files":{"config.json":{"bytes":null},"tokenizer.json":{"bytes":2},"model.safetensors":{"bytes":3}}}"#,
            r#"{"files":{"config.json":{},"tokenizer.json":{"bytes":2},"model.safetensors":{"bytes":3}}}"#,
        ] {
            let root = scratch("manifest-junk");
            let dir = write_model(&root, "m");
            write_manifest(&dir, body);

            let status = ModelLocator::new([root]).locate("m");
            match &status {
                ModelStatus::Absent { rejected, .. } => assert!(
                    matches!(rejected[0].reason, Reason::BadManifest(_)),
                    "manifest {body} should be BadManifest, got {:?}",
                    rejected[0].reason
                ),
                ModelStatus::Ready(l) => panic!(
                    "manifest {body} wrongly yielded Ready with {:?}",
                    l.integrity()
                ),
            }
        }
    }

    #[test]
    fn manifest_size_mismatch_rejects_and_reports_both_numbers() {
        let root = scratch("manifest-bad");
        let dir = write_model(&root, "m");
        write_manifest(
            &dir,
            r#"{"files":{"config.json":{"bytes":2},"tokenizer.json":{"bytes":2},
               "model.safetensors":{"bytes":999999}}}"#,
        );

        let status = ModelLocator::new([root]).locate("m");
        match &status {
            ModelStatus::Absent { rejected, .. } => match &rejected[0].reason {
                Reason::SizeMismatch(m) => {
                    assert_eq!(m[0].file, "model.safetensors");
                    assert_eq!(m[0].declared, 999_999);
                }
                other => panic!("expected SizeMismatch, got {other:?}"),
            },
            other => panic!("expected Absent, got {other:?}"),
        }
    }

    #[test]
    fn oversized_manifest_is_refused_rather_than_read() {
        let root = scratch("manifest-huge");
        let dir = write_model(&root, "m");
        write_manifest(&dir, &" ".repeat(MANIFEST_MAX_BYTES as usize + 1));

        let status = ModelLocator::new([root]).locate("m");
        match &status {
            ModelStatus::Absent { rejected, .. } => match &rejected[0].reason {
                Reason::BadManifest(e) => assert!(e.contains("exceeds")),
                other => panic!("expected BadManifest, got {other:?}"),
            },
            other => panic!("expected Absent, got {other:?}"),
        }
    }

    #[test]
    fn corrupt_manifest_is_an_error_not_a_silent_downgrade() {
        let root = scratch("manifest-corrupt");
        let dir = write_model(&root, "m");
        write_manifest(&dir, "{not json");

        let status = ModelLocator::new([root]).locate("m");
        match &status {
            ModelStatus::Absent { rejected, .. } => {
                assert!(matches!(rejected[0].reason, Reason::BadManifest(_)))
            }
            other => panic!("expected Absent, got {other:?}"),
        }
    }

    #[test]
    fn a_working_later_root_still_discloses_the_broken_earlier_one() {
        let bad = scratch("shadow-bad");
        let good = scratch("shadow-good");
        fs::create_dir_all(bad.join("m")).unwrap();
        fs::write(bad.join("m").join("config.json"), b"{}").unwrap();
        write_model(&good, "m");

        let status = ModelLocator::new([bad.clone(), good.clone()]).locate("m");
        let l = status.located().expect("Ready");
        assert_eq!(l.root(), good);
        assert_eq!(l.shadowed().len(), 1);
        assert_eq!(l.shadowed()[0].dir, bad.join("m"));
        assert!(status.summary().contains("WARNING"));
    }

    #[test]
    fn a_merely_absent_earlier_root_is_not_reported_as_a_broken_copy() {
        // NoDirectory is routine. Calling it "an unusable copy" trains the
        // operator to ignore the warning that matters.
        let empty = scratch("quiet-empty");
        let good = scratch("quiet-good");
        write_model(&good, "m");

        let status = ModelLocator::new([empty, good]).locate("m");
        let l = status.located().expect("Ready");
        assert_eq!(l.shadowed().len(), 1, "the skip is still recorded");
        assert!(!l.shadowed()[0].is_alarming());
        assert!(
            !status.summary().contains("WARNING"),
            "summary was: {}",
            status.summary()
        );
    }

    #[test]
    fn root_list_is_capped_deduped_and_the_truncation_is_disclosed() {
        let roots: Vec<PathBuf> = (0..MAX_ROOTS + 5)
            .map(|i| PathBuf::from(format!("/nonexistent/root{i}")))
            .collect();
        let loc = ModelLocator::new(roots);
        assert_eq!(loc.roots().len(), MAX_ROOTS);
        assert!(loc.locate("m").summary().contains("truncated"));

        // Duplicates must not crowd out real roots.
        let dupes: Vec<PathBuf> = std::iter::repeat_n(PathBuf::from("/same"), 20)
            .chain([PathBuf::from("/distinct")])
            .collect();
        let loc = ModelLocator::new(dupes);
        assert_eq!(loc.roots().len(), 2);
        assert!(!loc.locate("m").summary().contains("truncated"));
    }

    #[test]
    fn an_unbounded_root_iterator_does_not_hang_or_exhaust_memory() {
        // The cap must apply to the ITERATOR. Collecting first would never
        // return here.
        let endless = (0..).map(|i| PathBuf::from(format!("/endless/{i}")));
        let loc = ModelLocator::new(endless);
        assert_eq!(loc.roots().len(), MAX_ROOTS);
        assert!(loc.locate("m").summary().contains("truncated"));
    }

    #[test]
    fn ready_status_also_discloses_root_truncation() {
        // Absent disclosed it; Ready silently dropped it.
        let good = scratch("ready-trunc");
        write_model(&good, "m");
        let roots: Vec<PathBuf> = std::iter::once(good)
            .chain((0..MAX_ROOTS + 3).map(|i| PathBuf::from(format!("/nonexistent/r{i}"))))
            .collect();

        let status = ModelLocator::new(roots).locate("m");
        assert!(status.is_ready());
        assert!(
            status.summary().contains("truncated"),
            "summary was: {}",
            status.summary()
        );
    }

    #[test]
    fn traversing_model_ids_are_refused_before_any_path_is_built() {
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
            match &loc.locate(bad) {
                ModelStatus::Absent { rejected, .. } => assert_eq!(
                    rejected[0].reason,
                    Reason::UnsafeModelId,
                    "expected {bad:?} to be refused"
                ),
                other => panic!("expected Absent for {bad:?}, got {other:?}"),
            }
        }

        assert!(loc.locate("m").is_ready());
        assert!(model_id_is_safe("bge-small-en-v1.5"));
        assert!(model_id_is_safe("all_MiniLM-L6-v2"));
        assert!(!model_id_is_safe(&"a".repeat(129)));
    }

    #[test]
    fn a_rejected_model_id_is_bounded_and_control_stripped_in_output() {
        // The id is echoed back to the operator. A gigabyte id must not become
        // a gigabyte allocation, and an embedded newline must not be able to
        // forge additional status lines.
        let loc = ModelLocator::new([PathBuf::from("/nowhere")]);

        let huge = "a".repeat(100_000);
        let s = loc.locate(&huge).summary();
        assert!(s.len() < 500, "summary grew with the id: {} chars", s.len());
        assert!(s.contains("100000 chars total"));

        let s = loc.locate("evil\nWARNING: everything is fine").summary();
        assert!(!s.contains('\n'), "control characters survived: {s:?}");
    }

    /// End-to-end against REAL weights: the locator must hand candle a
    /// directory candle can actually load. A resolver whose output the loader
    /// rejects is worse than no resolver, and unit tests over synthetic files
    /// only prove the locator agrees with itself.
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
        let located = match status.located() {
            Some(l) => l,
            None => panic!("locator found nothing: {}", status.summary()),
        };

        // Take the id from the RESOLVED model, not a separate config value —
        // that is what stops MiniLM weights being scored against
        // bge-calibrated thresholds.
        let bert = CandleBert::load(located.dir(), Pooling::Cls, located.model_id())
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

        for v in &vecs {
            let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
            assert!((norm - 1.0).abs() < 1e-3, "vector not unit-length: {norm}");
        }

        let dot = |a: &[f32], b: &[f32]| a.iter().zip(b).map(|(x, y)| x * y).sum::<f32>();
        let related = dot(&vecs[0], &vecs[1]);
        let unrelated = dot(&vecs[0], &vecs[2]);
        assert!(
            related > unrelated,
            "two weather queries ({related:.3}) should be closer than weather vs antenna \
             hardware ({unrelated:.3}) — a loaded-but-wrong model still yields unit vectors"
        );
    }

    #[test]
    fn env_precedence_override_then_xdg_then_system() {
        assert_eq!(
            default_roots(Some("/opt/a:/opt/b"), Some("/xdg"), Some("/home/op")),
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
        assert_eq!(
            default_roots(Some("/opt/a::  :"), None, Some("/home/op")),
            vec![
                PathBuf::from("/opt/a"),
                PathBuf::from("/home/op/.local/share/tuxlink/models"),
                PathBuf::from("/usr/share/tuxlink/models"),
            ]
        );
    }

    #[test]
    fn system_root_is_always_last_even_with_no_env_at_all() {
        assert_eq!(
            default_roots(None, None, None),
            vec![PathBuf::from("/usr/share/tuxlink/models")]
        );
    }
}
