//! The weights-acquisition pipeline: stream → hash → verify against the
//! release pins → install (tuxlink-13ofm).
//!
//! ONE pipeline for every way bytes can arrive. A GitHub release asset, a
//! custom URL, and a USB-stick folder all flow through the same
//! stage-to-`.part` → stream-hash → compare-to-pin → atomic-rename sequence,
//! so "sideload is exactly as verified as the download" is true by
//! construction, not by parallel implementations agreeing.
//!
//! What the verification DOES claim: every byte that ends up under a required
//! file's final name hashed to the sha256 the application release pins
//! ([`tuxlink_classify::pins`]). A mismatch is refused BY NAME and the bytes
//! are removed. What it does NOT claim: protection from an attacker who can
//! already write to the models directory — that attacker can replace the
//! binary and its pins too (the same accepted posture as
//! `tuxlink_classify::hosting`).
//!
//! Interruption model: a transfer that dies mid-file leaves `<name>.part`,
//! whose length is the resume offset. On resume the existing prefix is
//! re-hashed (the digest must cover every byte, and hash state is not
//! persisted) and the source is asked for the remainder — HTTP via a `Range`
//! header, a folder via seek. A source that ignores `Range` restarts the file
//! cleanly rather than corrupting the hash.

use std::io::Write as _;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use sha2::{Digest, Sha256};
use tuxlink_classify::pins::{PinnedFile, PinnedModel};

use super::job::{FailureClass, JobRecord, Phase, Source};

/// Abort a transfer whose next chunk takes longer than this. A stalled TCP
/// stream otherwise hangs the job forever with no failure to retry from
/// (the basemap downloader's stall watchdog is the precedent).
const CHUNK_TIMEOUT: Duration = Duration::from_secs(60);

/// Read granularity for local-file streaming and prefix re-hashing.
const FILE_CHUNK: usize = 256 * 1024;

/// Extra bytes of free space demanded beyond the payload itself.
const FREE_SPACE_SLACK: u64 = 8 * 1024 * 1024;

/// Staging suffix beside the final file name.
pub const PART_SUFFIX: &str = ".part";

/// How one pipeline step failed, before it is folded into the job record.
#[derive(Debug)]
pub enum StepFail {
    /// Transient transport trouble — retried with backoff.
    Net(String),
    /// The source is serving the wrong thing — operator decision needed.
    Src(String),
    /// Completed bytes hashed to the wrong digest — poison, removed.
    Digest(String),
    /// Local filesystem failure.
    Io(String),
    Cancelled,
}

impl StepFail {
    pub fn class(&self) -> FailureClass {
        match self {
            StepFail::Net(_) => FailureClass::Network,
            StepFail::Src(_) => FailureClass::Source,
            StepFail::Digest(_) => FailureClass::DigestMismatch,
            StepFail::Io(_) => FailureClass::Io,
            StepFail::Cancelled => FailureClass::Cancelled,
        }
    }
    pub fn detail(&self) -> String {
        match self {
            StepFail::Net(d) | StepFail::Src(d) | StepFail::Digest(d) | StepFail::Io(d) => {
                d.clone()
            }
            StepFail::Cancelled => "cancelled".to_string(),
        }
    }
}

/// Everything the runner needs from its host. The command layer supplies
/// persistence + event emission through `announce`; tests supply counters.
pub struct RunCtx<'a> {
    pub models_root: &'a Path,
    pub model: &'a PinnedModel,
    pub cancel: &'a AtomicBool,
    /// Live byte progress: (file, hashed_or_received, file_total).
    pub on_progress: &'a (dyn Fn(&str, u64, u64) + Send + Sync),
    /// Phase changed: persist the record and tell the UI. Called on every
    /// record mutation the operator should see.
    pub announce: &'a (dyn Fn(&JobRecord) + Send + Sync),
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn hex(digest: &[u8]) -> String {
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// Short display form of a 64-hex digest for operator-facing messages.
fn short(d: &str) -> String {
    d.chars().take(12).collect::<String>() + "…"
}

fn set_phase(ctx: &RunCtx<'_>, record: &mut JobRecord, phase: Phase) {
    record.phase = phase;
    record.updated_unix = now_unix();
    (ctx.announce)(record);
}

/// Free bytes on the filesystem holding `path` (statvfs), `None` when it
/// cannot be determined — which callers treat as "do not proceed", same
/// contract as the basemap pre-flight.
fn available_bytes(path: &Path) -> Option<u64> {
    match nix::sys::statvfs::statvfs(path) {
        Ok(s) => {
            let blocks: u64 = s.blocks_available();
            let frag: u64 = s.fragment_size();
            Some(blocks.saturating_mul(frag))
        }
        Err(_) => None,
    }
}

/// One byte stream, source-agnostic.
enum ByteStream {
    Http(reqwest::Response),
    File(std::fs::File),
}

impl ByteStream {
    async fn next_chunk(&mut self) -> Result<Option<Vec<u8>>, StepFail> {
        match self {
            ByteStream::Http(resp) => {
                match tokio::time::timeout(CHUNK_TIMEOUT, resp.chunk()).await {
                    Err(_) => Err(StepFail::Net(format!(
                        "transfer stalled for {}s",
                        CHUNK_TIMEOUT.as_secs()
                    ))),
                    Ok(Err(e)) => Err(StepFail::Net(format!("transfer broke: {e}"))),
                    Ok(Ok(Some(bytes))) => Ok(Some(bytes.to_vec())),
                    Ok(Ok(None)) => Ok(None),
                }
            }
            ByteStream::File(f) => {
                use std::io::Read as _;
                let mut buf = vec![0u8; FILE_CHUNK];
                match f.read(&mut buf) {
                    Ok(0) => Ok(None),
                    Ok(n) => {
                        buf.truncate(n);
                        Ok(Some(buf))
                    }
                    Err(e) => Err(StepFail::Io(format!("read source file: {e}"))),
                }
            }
        }
    }
}

/// What `open_stream` learned when it opened the source.
struct Opened {
    stream: ByteStream,
    /// Whether the source honored the requested offset. `false` means the
    /// caller must restart the file from zero (truncate + fresh hasher).
    resumed: bool,
}

/// Redirect hop filter: a chain that starts https must never be able to hop
/// to plain http on a non-loopback host (the forms-updater posture). Content
/// is digest-pinned so this is defense in depth, not the integrity mechanism.
fn redirect_hop_allowed(next: &reqwest::Url) -> bool {
    if next.scheme() == "https" {
        return true;
    }
    matches!(next.host_str(), Some("localhost") | Some("127.0.0.1") | Some("[::1]"))
}

fn http_client() -> Result<reqwest::Client, StepFail> {
    reqwest::Client::builder()
        .user_agent(concat!("tuxlink/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() > 8 {
                attempt.error("too many redirects")
            } else if redirect_hop_allowed(attempt.url()) {
                attempt.follow()
            } else {
                attempt.error("refusing redirect to plain http on a non-loopback host")
            }
        }))
        .build()
        .map_err(|e| StepFail::Net(format!("http client: {e}")))
}

async fn open_stream(
    source: &Source,
    model: &PinnedModel,
    pin: &PinnedFile,
    offset: u64,
) -> Result<Opened, StepFail> {
    match source {
        Source::Release { base_url } => {
            let url = format!(
                "{}/{}",
                base_url.trim_end_matches('/'),
                model.asset_name(pin)
            );
            let client = http_client()?;
            let mut req = client.get(&url);
            if offset > 0 {
                req = req.header(reqwest::header::RANGE, format!("bytes={offset}-"));
            }
            let resp = match tokio::time::timeout(CHUNK_TIMEOUT, req.send()).await {
                Err(_) => return Err(StepFail::Net(format!("no response from {url}"))),
                Ok(Err(e)) => return Err(StepFail::Net(format!("request to {url} failed: {e}"))),
                Ok(Ok(resp)) => resp,
            };

            let status = resp.status();
            let resumed = status == reqwest::StatusCode::PARTIAL_CONTENT && offset > 0;
            if !(status.is_success() || resumed) {
                // 5xx / 429 are worth retrying; anything else 4xx means the
                // source does not have (or will not serve) this asset.
                let msg = format!("{url}: HTTP {status}");
                return if status.is_server_error()
                    || status == reqwest::StatusCode::TOO_MANY_REQUESTS
                {
                    Err(StepFail::Net(msg))
                } else {
                    Err(StepFail::Src(format!(
                        "{msg} — this source has no usable '{}' (a release without \
                         weight assets, or a wrong base URL); switch source or install \
                         from a folder",
                        model.asset_name(pin)
                    )))
                };
            }

            // Size sanity BEFORE the transfer: a source whose Content-Length
            // cannot match the pin will never verify; refuse without spending
            // the bytes. (An absent Content-Length is tolerated — the running
            // cap below still bounds the stream.)
            let expected_remaining = pin.bytes - if resumed { offset } else { 0 };
            if let Some(len) = resp.content_length() {
                if len != expected_remaining {
                    return Err(StepFail::Src(format!(
                        "{}: source offers {len} bytes where the release pins {} — \
                         wrong file at this source",
                        pin.name, expected_remaining
                    )));
                }
            }

            Ok(Opened {
                stream: ByteStream::Http(resp),
                resumed,
            })
        }
        Source::Sideload { dir } => {
            use std::io::Seek as _;
            let src = dir.join(pin.name);
            let meta = std::fs::metadata(&src)
                .map_err(|e| StepFail::Src(format!("{}: {e}", src.display())))?;
            if !meta.is_file() {
                return Err(StepFail::Src(format!("{} is not a file", src.display())));
            }
            if meta.len() != pin.bytes {
                return Err(StepFail::Src(format!(
                    "{}: folder copy is {} bytes where the release pins {} — \
                     wrong or truncated file",
                    pin.name,
                    meta.len(),
                    pin.bytes
                )));
            }
            let mut f = std::fs::File::open(&src)
                .map_err(|e| StepFail::Io(format!("open {}: {e}", src.display())))?;
            f.seek(std::io::SeekFrom::Start(offset))
                .map_err(|e| StepFail::Io(format!("seek {}: {e}", src.display())))?;
            Ok(Opened {
                stream: ByteStream::File(f),
                resumed: true,
            })
        }
    }
}

/// Hash a whole existing file, streaming progress. Used to decide whether an
/// already-present final file can be kept without touching the source.
fn hash_existing(
    path: &Path,
    total: u64,
    file_label: &str,
    ctx: &RunCtx<'_>,
) -> Result<String, StepFail> {
    use std::io::Read as _;
    let mut f =
        std::fs::File::open(path).map_err(|e| StepFail::Io(format!("open {}: {e}", path.display())))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; FILE_CHUNK];
    let mut got: u64 = 0;
    loop {
        if ctx.cancel.load(Ordering::Relaxed) {
            return Err(StepFail::Cancelled);
        }
        let n = f
            .read(&mut buf)
            .map_err(|e| StepFail::Io(format!("read {}: {e}", path.display())))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        got += n as u64;
        (ctx.on_progress)(file_label, got, total);
    }
    Ok(hex(&hasher.finalize()))
}

/// Acquire one pinned file into `dir`, honoring an existing final copy or a
/// resumable `.part`. Returns `Ok(())` with the verified file installed under
/// its final name.
async fn acquire_file(
    ctx: &RunCtx<'_>,
    record: &mut JobRecord,
    dir: &Path,
    pin: &PinnedFile,
) -> Result<(), StepFail> {
    let final_path = dir.join(pin.name);
    let part_path = dir.join(format!("{}{PART_SUFFIX}", pin.name));

    // An existing final copy that already matches the pin is kept — this is
    // what lets a retry after one bad file skip the two good ones, and a
    // half-sideloaded folder complete over the network.
    if final_path.is_file() {
        set_phase(
            ctx,
            record,
            Phase::Verifying {
                file: pin.name.to_string(),
            },
        );
        let digest = hash_existing(&final_path, pin.bytes, pin.name, ctx)?;
        if digest == pin.sha256 {
            return Ok(());
        }
        // Mismatched final copy: left in place; the verified replacement
        // lands over it atomically at rename time.
    }

    // Work out the resume offset and bring the hasher up to it.
    let mut hasher = Sha256::new();
    let mut got: u64 = 0;
    if part_path.is_file() {
        let len = std::fs::metadata(&part_path)
            .map_err(|e| StepFail::Io(format!("stat {}: {e}", part_path.display())))?
            .len();
        if len > pin.bytes {
            // Cannot be a prefix of the pinned content.
            std::fs::remove_file(&part_path)
                .map_err(|e| StepFail::Io(format!("remove {}: {e}", part_path.display())))?;
        } else if len > 0 {
            use std::io::Read as _;
            set_phase(
                ctx,
                record,
                Phase::Verifying {
                    file: pin.name.to_string(),
                },
            );
            let mut f = std::fs::File::open(&part_path)
                .map_err(|e| StepFail::Io(format!("open {}: {e}", part_path.display())))?;
            let mut buf = vec![0u8; FILE_CHUNK];
            loop {
                if ctx.cancel.load(Ordering::Relaxed) {
                    return Err(StepFail::Cancelled);
                }
                let n = f
                    .read(&mut buf)
                    .map_err(|e| StepFail::Io(format!("read {}: {e}", part_path.display())))?;
                if n == 0 {
                    break;
                }
                hasher.update(&buf[..n]);
                got += n as u64;
                (ctx.on_progress)(pin.name, got, pin.bytes);
            }
        }
    }

    set_phase(
        ctx,
        record,
        Phase::Downloading {
            file: pin.name.to_string(),
        },
    );

    let opened = open_stream(&record.source, ctx.model, pin, got).await?;
    let mut stream = opened.stream;
    if !opened.resumed && got > 0 {
        // Source ignored the offset (plain 200): restart the file cleanly.
        hasher = Sha256::new();
        got = 0;
    }

    let mut out = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(!opened.resumed || got == 0)
        .append(opened.resumed && got > 0)
        .open(&part_path)
        .map_err(|e| StepFail::Io(format!("open {}: {e}", part_path.display())))?;

    loop {
        if ctx.cancel.load(Ordering::Relaxed) {
            return Err(StepFail::Cancelled);
        }
        let chunk = match stream.next_chunk().await? {
            Some(c) => c,
            None => break,
        };
        got += chunk.len() as u64;
        if got > pin.bytes {
            // The pin bounds the file; a source streaming past it can never
            // verify and gets cut off rather than filling the disk.
            drop(out);
            let _ = std::fs::remove_file(&part_path);
            return Err(StepFail::Src(format!(
                "{}: source streamed past the pinned {} bytes — wrong file at this source",
                pin.name, pin.bytes
            )));
        }
        hasher.update(&chunk);
        out.write_all(&chunk)
            .map_err(|e| StepFail::Io(format!("write {}: {e}", part_path.display())))?;
        (ctx.on_progress)(pin.name, got, pin.bytes);
    }

    if got != pin.bytes {
        // Short stream: keep the .part — it is a valid prefix and the whole
        // point of the resume machinery.
        return Err(StepFail::Net(format!(
            "{}: transfer ended early at {got} of {} bytes",
            pin.name, pin.bytes
        )));
    }

    set_phase(
        ctx,
        record,
        Phase::Verifying {
            file: pin.name.to_string(),
        },
    );
    let digest = hex(&hasher.finalize());
    if digest != pin.sha256 {
        // Poison: full-length but wrong content. Remove it — leaving it
        // invites a manual rename into place, which is exactly the bypass
        // the pins exist to prevent.
        drop(out);
        let _ = std::fs::remove_file(&part_path);
        return Err(StepFail::Digest(format!(
            "{}: downloaded bytes hash to {} where this release pins {} — refused and removed \
             (the source is serving different content than this version of Tuxlink ships \
             verification for)",
            pin.name,
            short(&digest),
            short(pin.sha256)
        )));
    }

    out.sync_all()
        .map_err(|e| StepFail::Io(format!("sync {}: {e}", part_path.display())))?;
    drop(out);
    std::fs::rename(&part_path, &final_path)
        .map_err(|e| StepFail::Io(format!("install {}: {e}", final_path.display())))?;
    Ok(())
}

/// Write the model directory's `manifest.json`: the byte lengths the locator
/// verifies on every later boot, plus a provenance stanza recording that this
/// install was digest-verified against this release's pins. The locator
/// ignores the extra stanza (extra keys are tolerated by contract); the
/// status surface reads it.
fn write_manifest(dir: &Path, model: &PinnedModel) -> Result<(), StepFail> {
    let files: serde_json::Map<String, serde_json::Value> = model
        .files
        .iter()
        .map(|f| {
            (
                f.name.to_string(),
                serde_json::json!({ "bytes": f.bytes }),
            )
        })
        .collect();
    let doc = serde_json::json!({
        "files": files,
        "verified": {
            "method": "sha256-release-pins",
            "release": env!("CARGO_PKG_VERSION"),
            "at_unix": now_unix(),
        },
    });
    let tmp = dir.join("manifest.json.tmp");
    let path = dir.join(tuxlink_classify::hosting::MANIFEST_FILE);
    std::fs::write(&tmp, serde_json::to_string_pretty(&doc).expect("static shape"))
        .map_err(|e| StepFail::Io(format!("write {}: {e}", tmp.display())))?;
    std::fs::rename(&tmp, &path)
        .map_err(|e| StepFail::Io(format!("rename {}: {e}", tmp.display())))?;
    Ok(())
}

/// One full acquisition attempt over every required file. Returns the final
/// phase to record. Idempotent: already-verified files are kept, a `.part`
/// resumes, and only the remainder is fetched.
pub async fn run_once(ctx: &RunCtx<'_>, record: &mut JobRecord) -> Phase {
    let dir = ctx.models_root.join(ctx.model.model_id);
    if let Err(e) = std::fs::create_dir_all(&dir) {
        return Phase::Failed {
            detail: format!("create {}: {e}", dir.display()),
            class: FailureClass::Io,
        };
    }

    // Free-space pre-flight against the whole payload (files already present
    // only make this conservative). `None` — an undeterminable filesystem —
    // refuses, per the basemap contract.
    let need = ctx.model.total_bytes() + FREE_SPACE_SLACK;
    match available_bytes(&dir) {
        Some(avail) if avail >= need => {}
        Some(avail) => {
            return Phase::Failed {
                detail: format!(
                    "not enough space in {}: need ~{} MB, {} MB available",
                    dir.display(),
                    need / (1024 * 1024),
                    avail / (1024 * 1024)
                ),
                class: FailureClass::Io,
            };
        }
        None => {
            return Phase::Failed {
                detail: format!(
                    "could not determine free space in {} — refusing to start a transfer \
                     onto an unknown filesystem",
                    dir.display()
                ),
                class: FailureClass::Io,
            };
        }
    }

    for pin in &ctx.model.files {
        if record.files_done.iter().any(|f| f == pin.name) {
            continue;
        }
        match acquire_file(ctx, record, &dir, pin).await {
            Ok(()) => {
                record.files_done.push(pin.name.to_string());
                record.updated_unix = now_unix();
                (ctx.announce)(record);
            }
            Err(fail) => {
                return Phase::Failed {
                    detail: fail.detail(),
                    class: fail.class(),
                };
            }
        }
    }

    if let Err(fail) = write_manifest(&dir, ctx.model) {
        return Phase::Failed {
            detail: fail.detail(),
            class: fail.class(),
        };
    }

    Phase::Complete { at_unix: now_unix() }
}

/// Backoff schedule for automatic network retries: patient, capped, and
/// derived from the attempt count rather than any invented deadline.
pub fn backoff_secs(attempt: u32) -> u64 {
    let base = 15u64.saturating_mul(1u64 << attempt.min(6));
    base.min(600)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::AtomicU64;
    use std::sync::Mutex;

    /// A tiny stand-in for the real pinned model: same shape, test-sized
    /// content, digests precomputed from the fixed bytes below.
    const T_CONFIG: &[u8] = b"tuxlink-test-config";
    const T_TOKENIZER: &[u8] = b"tuxlink-test-tokenizer";
    const T_WEIGHTS: &[u8] = b"tuxlink-test-weights";

    fn test_model() -> PinnedModel {
        PinnedModel {
            model_id: "test-model",
            files: [
                tuxlink_classify::pins::PinnedFile {
                    name: "config.json",
                    bytes: T_CONFIG.len() as u64,
                    sha256: "7fa1e634df325c4d67f692dbc7bb01a6e3e7c653199810e9e231b564af7e34bf",
                },
                tuxlink_classify::pins::PinnedFile {
                    name: "tokenizer.json",
                    bytes: T_TOKENIZER.len() as u64,
                    sha256: "503c89d5b0cb652d65a1ad3daa70a4f3d51b0e5add4e8f77893f92308e7e5def",
                },
                tuxlink_classify::pins::PinnedFile {
                    name: "model.safetensors",
                    bytes: T_WEIGHTS.len() as u64,
                    sha256: "032e76f4684f29e7874ae142180b0506f51c2ab42070e53b5708a274e73e2997",
                },
            ],
        }
    }

    fn scratch(tag: &str) -> PathBuf {
        use std::sync::atomic::AtomicUsize;
        static N: AtomicUsize = AtomicUsize::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "tuxlink-acquire-{}-{tag}-{n}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_source_dir(dir: &Path) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("config.json"), T_CONFIG).unwrap();
        std::fs::write(dir.join("tokenizer.json"), T_TOKENIZER).unwrap();
        std::fs::write(dir.join("model.safetensors"), T_WEIGHTS).unwrap();
    }

    struct Probe {
        progress_calls: AtomicU64,
        announced: Mutex<Vec<String>>,
    }

    impl Probe {
        fn new() -> Self {
            Probe {
                progress_calls: AtomicU64::new(0),
                announced: Mutex::new(Vec::new()),
            }
        }
    }

    fn run(
        models_root: &Path,
        model: &PinnedModel,
        record: &mut JobRecord,
        cancel: &AtomicBool,
        probe: &Probe,
    ) -> Phase {
        let on_progress = |_f: &str, _got: u64, _total: u64| {
            probe.progress_calls.fetch_add(1, Ordering::Relaxed);
        };
        let announce = |r: &JobRecord| {
            probe
                .announced
                .lock()
                .unwrap()
                .push(format!("{:?}", r.phase));
        };
        let ctx = RunCtx {
            models_root,
            model,
            cancel,
            on_progress: &on_progress,
            announce: &announce,
        };
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap()
            .block_on(run_once(&ctx, record))
    }

    fn sideload_record(src: &Path) -> JobRecord {
        JobRecord::new(
            "test-model",
            Source::Sideload {
                dir: src.to_path_buf(),
            },
            1,
        )
    }

    #[test]
    fn sideload_happy_path_installs_and_manifest_records_digest_provenance() {
        let root = scratch("happy");
        let src = scratch("happy-src");
        write_source_dir(&src);
        let model = test_model();
        let mut rec = sideload_record(&src);
        let probe = Probe::new();

        let phase = run(&root, &model, &mut rec, &AtomicBool::new(false), &probe);

        assert!(matches!(phase, Phase::Complete { .. }), "{phase:?}");
        assert_eq!(rec.files_done.len(), 3);
        let dir = root.join("test-model");
        for pin in &model.files {
            assert_eq!(std::fs::read(dir.join(pin.name)).unwrap().len() as u64, pin.bytes);
            assert!(!dir.join(format!("{}{PART_SUFFIX}", pin.name)).exists());
        }
        let manifest: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("manifest.json")).unwrap())
                .unwrap();
        assert_eq!(manifest["verified"]["method"], "sha256-release-pins");
        assert_eq!(manifest["verified"]["release"], env!("CARGO_PKG_VERSION"));
        assert!(probe.progress_calls.load(Ordering::Relaxed) > 0);

        // The locator — the component that answers "is T1 usable" — accepts
        // the result and reports the manifest-backed integrity tier.
        let status = tuxlink_classify::hosting::ModelLocator::new([root.clone()])
            .locate("test-model");
        let located = status.located().expect("locator should accept the install");
        assert_eq!(
            located.integrity(),
            tuxlink_classify::hosting::Integrity::SizeVerified
        );
    }

    #[test]
    fn corrupt_source_file_is_refused_by_name_and_nothing_installs() {
        let root = scratch("corrupt");
        let src = scratch("corrupt-src");
        write_source_dir(&src);
        // Same length, different content — the case size checks cannot catch.
        std::fs::write(src.join("model.safetensors"), b"tuxlink-EVIL-weights").unwrap();
        let model = test_model();
        let mut rec = sideload_record(&src);
        let probe = Probe::new();

        let phase = run(&root, &model, &mut rec, &AtomicBool::new(false), &probe);

        match phase {
            Phase::Failed { detail, class } => {
                assert_eq!(class, FailureClass::DigestMismatch);
                assert!(detail.contains("model.safetensors"), "{detail}");
                assert!(detail.contains("032e76f4684f"), "{detail}");
            }
            other => panic!("expected DigestMismatch, got {other:?}"),
        }
        let dir = root.join("test-model");
        assert!(!dir.join("model.safetensors").exists());
        assert!(!dir.join("model.safetensors.part").exists(), "poison must be removed");
        assert!(!dir.join("manifest.json").exists());
        // The two good files DID install — a retry only needs the bad one.
        assert_eq!(rec.files_done, vec!["config.json", "tokenizer.json"]);
    }

    #[test]
    fn wrong_size_at_source_fails_early_as_a_source_problem() {
        let root = scratch("size");
        let src = scratch("size-src");
        write_source_dir(&src);
        std::fs::write(src.join("tokenizer.json"), b"short").unwrap();
        let model = test_model();
        let mut rec = sideload_record(&src);
        let probe = Probe::new();

        let phase = run(&root, &model, &mut rec, &AtomicBool::new(false), &probe);

        match phase {
            Phase::Failed { detail, class } => {
                assert_eq!(class, FailureClass::Source);
                assert!(detail.contains("tokenizer.json"), "{detail}");
            }
            other => panic!("expected Source failure, got {other:?}"),
        }
    }

    #[test]
    fn missing_source_file_names_it_and_is_not_auto_retryable() {
        let root = scratch("missing");
        let src = scratch("missing-src");
        write_source_dir(&src);
        std::fs::remove_file(src.join("config.json")).unwrap();
        let model = test_model();
        let mut rec = sideload_record(&src);
        let probe = Probe::new();

        let phase = run(&root, &model, &mut rec, &AtomicBool::new(false), &probe);

        match phase {
            Phase::Failed { detail, class } => {
                assert_eq!(class, FailureClass::Source);
                assert!(detail.contains("config.json"), "{detail}");
                assert!(!class.auto_retryable());
            }
            other => panic!("expected Source failure, got {other:?}"),
        }
    }

    #[test]
    fn valid_existing_files_are_kept_without_touching_the_source() {
        let root = scratch("keep");
        let dir = root.join("test-model");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("config.json"), T_CONFIG).unwrap();
        std::fs::write(dir.join("tokenizer.json"), T_TOKENIZER).unwrap();
        std::fs::write(dir.join("model.safetensors"), T_WEIGHTS).unwrap();
        // The source directory is EMPTY: if the pipeline reached for it,
        // every file would fail as missing.
        let src = scratch("keep-src-empty");
        let model = test_model();
        let mut rec = sideload_record(&src);
        let probe = Probe::new();

        let phase = run(&root, &model, &mut rec, &AtomicBool::new(false), &probe);

        assert!(matches!(phase, Phase::Complete { .. }), "{phase:?}");
    }

    #[test]
    fn a_part_prefix_resumes_instead_of_restarting() {
        let root = scratch("resume");
        let dir = root.join("test-model");
        std::fs::create_dir_all(&dir).unwrap();
        // First half of the weights already on disk from a dead transfer.
        let half = T_WEIGHTS.len() / 2;
        std::fs::write(dir.join("model.safetensors.part"), &T_WEIGHTS[..half]).unwrap();
        let src = scratch("resume-src");
        write_source_dir(&src);
        let model = test_model();
        let mut rec = sideload_record(&src);
        let probe = Probe::new();

        let phase = run(&root, &model, &mut rec, &AtomicBool::new(false), &probe);

        assert!(matches!(phase, Phase::Complete { .. }), "{phase:?}");
        assert_eq!(std::fs::read(dir.join("model.safetensors")).unwrap(), T_WEIGHTS);
    }

    #[test]
    fn an_oversized_part_is_discarded_and_the_file_refetched() {
        let root = scratch("overpart");
        let dir = root.join("test-model");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("model.safetensors.part"),
            b"tuxlink-test-weightsEXTRA",
        )
        .unwrap();
        let src = scratch("overpart-src");
        write_source_dir(&src);
        let model = test_model();
        let mut rec = sideload_record(&src);
        let probe = Probe::new();

        let phase = run(&root, &model, &mut rec, &AtomicBool::new(false), &probe);

        assert!(matches!(phase, Phase::Complete { .. }), "{phase:?}");
        assert_eq!(std::fs::read(dir.join("model.safetensors")).unwrap(), T_WEIGHTS);
    }

    #[test]
    fn cancel_stops_the_run_and_keeps_partial_state_for_later() {
        let root = scratch("cancel");
        let src = scratch("cancel-src");
        write_source_dir(&src);
        let model = test_model();
        let mut rec = sideload_record(&src);
        let probe = Probe::new();

        let phase = run(&root, &model, &mut rec, &AtomicBool::new(true), &probe);

        match phase {
            Phase::Failed { class, .. } => assert_eq!(class, FailureClass::Cancelled),
            other => panic!("expected Cancelled, got {other:?}"),
        }
    }

    #[test]
    fn mismatched_final_file_is_replaced_atomically_by_a_verified_one() {
        let root = scratch("replace");
        let dir = root.join("test-model");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("config.json"), T_CONFIG).unwrap();
        std::fs::write(dir.join("tokenizer.json"), T_TOKENIZER).unwrap();
        // A stale/foreign weights file of the RIGHT length but WRONG content.
        std::fs::write(dir.join("model.safetensors"), b"tuxlink-EVIL-weights").unwrap();
        let src = scratch("replace-src");
        write_source_dir(&src);
        let model = test_model();
        let mut rec = sideload_record(&src);
        let probe = Probe::new();

        let phase = run(&root, &model, &mut rec, &AtomicBool::new(false), &probe);

        assert!(matches!(phase, Phase::Complete { .. }), "{phase:?}");
        assert_eq!(std::fs::read(dir.join("model.safetensors")).unwrap(), T_WEIGHTS);
    }

    #[test]
    fn backoff_grows_and_caps() {
        assert_eq!(backoff_secs(0), 15);
        assert_eq!(backoff_secs(1), 30);
        assert_eq!(backoff_secs(2), 60);
        assert_eq!(backoff_secs(6), 600);
        assert_eq!(backoff_secs(60), 600, "attempt count must not overflow the shift");
    }

    #[test]
    fn redirect_policy_allows_https_and_loopback_http_only() {
        let ok = |u: &str| redirect_hop_allowed(&reqwest::Url::parse(u).unwrap());
        assert!(ok("https://objects.githubusercontent.com/x"));
        assert!(ok("http://127.0.0.1:8080/x"));
        assert!(ok("http://localhost/x"));
        assert!(!ok("http://example.com/x"));
        assert!(!ok("http://192.168.1.10/x"), "private-range http still refused on redirect");
    }
}
