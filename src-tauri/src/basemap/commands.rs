//! Region-pack Tauri command surface + runtime glue (tuxlink-ndi4, phase 4 R-4).
//!
//! Design: docs/design/2026-06-13-ndi4-d1-region-pack-distribution.md
//!
//! Bridges the pure, tested core ([`super::region_manifest`], [`super::packs`],
//! [`super::download`]) to the webview. The webview calls these `invoke` commands;
//! the actual byte transfer is the go-pmtiles sidecar, invoked here via the
//! established `std::process::Command` pattern (managed_direwolf/rfcomm) on the
//! bundled `externalBin`. Every value that reaches the sidecar argv is already
//! locked down by the core: `planet_url` allowlisted (manifest parse), bbox
//! clamped/ordered ([`super::packs::tier_bbox`]), `--maxzoom` an app constant,
//! and `dest` an app-controlled path — so a hostile manifest cannot inject an
//! arg, flag, or SSRF target.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use super::download::{self, DownloadError, Extractor, PackRequest};
use super::packs::{self, Bbox, InstalledPack};
use super::region_manifest::RegionManifest;
use super::PmtilesRegistry;

/// Canonical manifest refresh source — see D1 §"manifest hosting". Fetched in
/// Rust (this command), never the webview, so the CSP stays closed.
const MANIFEST_URL: &str =
    "https://raw.githubusercontent.com/cameronzucker/tuxlink/main/src-tauri/resources/basemap/region-manifest.json";

/// `--maxzoom` for the AREA (operator-grid tier) pack path: a small box at full
/// detail (z0–14, D1). App constant on that path — never from the manifest. The
/// CONTINENT path is detail-tiered (tuxlink-8g28) and uses the selected tier's
/// `maxzoom` instead, which `RegionManifest::parse` bounds to `MAX_TIER_MAXZOOM`
/// (== this value), so it likewise can't be turned into an oversized extract.
const PACK_MAXZOOM: u8 = 14;

/// Per-zoom shrink factor for the continent size model ([`continent_estimate`]).
/// A continent extract one zoom below z14 is estimated at ~1/this the bytes. Vector
/// tiles grow ~2–3× per added zoom; we use the GENTLE end (2) so the estimate biases
/// HIGH — a too-low estimate would make `validate` (`size_budget = estimate * 3`)
/// reject a legitimate extract, whereas a high estimate only over-reserves free
/// space (the safe direction). The model is unmeasured; calibrate against a real
/// `pmtiles extract` if the progress bar / free-space gate proves off.
const CONTINENT_ZOOM_SHRINK: u64 = 2;

/// Stall watchdog: if the in-flight `.part` has not grown for this long, the
/// go-pmtiles sidecar is treated as hung (dead connection mid-extract) and killed,
/// so the blocking download thread unwinds and its in-flight guard clears. Without
/// this, a hung sidecar leaves the guard set forever and every Retry bounces with
/// "already in progress" (tuxlink-8g28). Generous so a slow-but-alive transfer —
/// which writes *something* well within this window — is never false-aborted.
const STALL_TIMEOUT: Duration = Duration::from_secs(180);

/// How often the sidecar poll loop wakes to check cancel + sample the temp size.
const POLL_INTERVAL: Duration = Duration::from_millis(500);

/// Progress-event emission throttle: at most one `basemap:download-progress` per
/// this window (the poll loop samples more often than the UI needs to repaint).
const EMIT_THROTTLE: Duration = Duration::from_millis(400);

/// Event channels for the pack-download progress UI (see `useDownloadProgress.ts`).
const PROGRESS_EVENT: &str = "basemap:download-progress";
const DONE_EVENT: &str = "basemap:download-done";

/// `basemap:download-progress` payload: live byte count + the expected total so
/// the UI can render a determinate bar, rate, and ETA. serde camelCase to match
/// the TS `DownloadProgress` type.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DownloadProgress {
    pack_id: String,
    bytes: u64,
    total: u64,
}

/// `basemap:download-done` payload: terminal signal so the UI clears the bar even
/// on failure/cancel (the command Result alone wouldn't reach a listener-based UI).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DownloadDone {
    pack_id: String,
    ok: bool,
    error: Option<String>,
}

/// Free-space safety margin over the manifest's `typical_bytes` estimate, and the
/// validation size budget multiplier (a real extract can exceed the estimate; the
/// budget still rejects a runaway).
const NEEDED_MARGIN_NUM: u64 = 6; // needed = typical * 6/5  (20% headroom)
const NEEDED_MARGIN_DEN: u64 = 5;
const BUDGET_MULT: u64 = 3; // size_budget = typical * 3 (generous; rejects absurd)

/// Managed runtime state: the live region manifest (bundled default until a
/// refresh succeeds) + the resolved packs directory.
pub struct BasemapState {
    pub manifest: RwLock<RegionManifest>,
    pub packs_dir: PathBuf,
    /// Serializes pack install/delete so the packs-manifest read-modify-write
    /// (load → upsert/remove → atomic write) can't lose an update under two
    /// concurrent downloads (Tauri dispatches sync commands on a thread pool). A
    /// lost update would leave a completed pack's archive unreferenced → the
    /// startup orphan-sweep would silently delete the just-downloaded pack.
    pub install_lock: Mutex<()>,
    /// Per-download cancel flags keyed by pack id. `basemap_download_pack` inserts
    /// a fresh `false` flag before extract and removes it after (success/err/
    /// cancel); `basemap_cancel_download` flips the flag so the in-flight extract's
    /// poll loop stops + kills the sidecar. Atomic install already guarantees a
    /// cancelled download leaves no installed pack (only the `.part`, swept).
    pub download_cancels: Mutex<HashMap<String, Arc<AtomicBool>>>,
}

impl BasemapState {
    pub fn new(packs_dir: PathBuf) -> Self {
        Self {
            manifest: RwLock::new(RegionManifest::bundled_default()),
            packs_dir,
            install_lock: Mutex::new(()),
            download_cancels: Mutex::new(HashMap::new()),
        }
    }
}

/// go-pmtiles via `std::process::Command` on the resolved bundled sidecar binary.
struct SidecarExtractor {
    bin: PathBuf,
}

impl Extractor for SidecarExtractor {
    fn extract(
        &self,
        planet_url: &str,
        bbox: &Bbox,
        maxzoom: u8,
        dest: &Path,
        cancel: &Arc<AtomicBool>,
        on_progress: &dyn Fn(u64),
    ) -> Result<(), DownloadError> {
        use std::process::{Command, Stdio};

        // No shell — argv tokens are passed directly to execvp. planet_url/bbox
        // are pre-validated; maxzoom/dest are app-controlled. We `.spawn()` (not
        // `.output()`) so the loop below can poll the cancel flag + the temp's
        // size for progress. BOTH stdout and stderr are captured for the error
        // message on a non-zero exit — go-pmtiles writes its real failure (e.g.
        // "Failed to create range reader ... HTTP error: 404" when the planet
        // build URL has rotated) to STDOUT, so dropping stdout left the user with
        // an empty, useless error. Progress still comes from polling `dest` size,
        // never from parsing sidecar output (robust + sidecar-agnostic).
        let mut child = Command::new(&self.bin)
            .arg("extract")
            .arg(planet_url)
            .arg(dest)
            .arg(format!("--maxzoom={maxzoom}"))
            .arg(format!("--bbox={}", bbox.to_arg()))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| DownloadError::ExtractFailed(format!("spawn go-pmtiles: {e}")))?;

        // Drain stdout + stderr on their own threads so a chatty sidecar can't
        // deadlock on a full pipe while we poll; joined after exit for the error
        // message. go-pmtiles emits its real error on stdout, diagnostics on
        // stderr — capture both and surface whichever is non-empty.
        fn drain(
            pipe: Option<impl std::io::Read + Send + 'static>,
        ) -> Option<std::thread::JoinHandle<String>> {
            pipe.map(|mut s| {
                std::thread::spawn(move || {
                    let mut buf = String::new();
                    let _ = s.read_to_string(&mut buf);
                    buf
                })
            })
        }
        let stdout_handle = drain(child.stdout.take());
        let stderr_handle = drain(child.stderr.take());

        // What the poll loop resolved to. The loop only decides the outcome and
        // breaks; the drain threads are joined ONCE after the loop on EVERY path
        // (Codex #3) — the prior code joined them only on a non-zero exit, leaking
        // both threads on success / cancel / try_wait-error. After a natural exit
        // or a kill+wait, the child's pipe write ends are closed, so the reads hit
        // EOF and the joins return promptly.
        enum Outcome {
            Exited(std::process::ExitStatus),
            Cancelled,
            Stalled,
            WaitErr(std::io::Error),
        }
        // Stall watchdog state (tuxlink-8g28): the last `.part` size we saw grow and
        // when. A hung sidecar (dead connection) stops growing the file but
        // try_wait() never returns Some — without this the loop spins forever, the
        // in-flight guard never clears, and Retry bounces with "already in progress".
        let mut last_size: u64 = 0;
        let mut last_growth = Instant::now();
        let outcome = loop {
            if cancel.load(Ordering::SeqCst) {
                let _ = child.kill();
                let _ = child.wait();
                break Outcome::Cancelled;
            }
            // Poll the temp size for the progress bar. A missing/locked file early
            // on just yields 0 — not an error.
            let written = std::fs::metadata(dest).map(|m| m.len()).unwrap_or(0);
            on_progress(written);

            // Reset the watchdog on any growth; trip it if the file has been static
            // for STALL_TIMEOUT while the child is still (apparently) running.
            if written > last_size {
                last_size = written;
                last_growth = Instant::now();
            } else if download::is_stalled(last_growth.elapsed(), STALL_TIMEOUT) {
                let _ = child.kill();
                let _ = child.wait();
                break Outcome::Stalled;
            }

            match child.try_wait() {
                Ok(Some(status)) => {
                    // Final size sample before reporting completion.
                    let written = std::fs::metadata(dest).map(|m| m.len()).unwrap_or(written);
                    on_progress(written);
                    break Outcome::Exited(status);
                }
                Ok(None) => std::thread::sleep(POLL_INTERVAL),
                Err(e) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    break Outcome::WaitErr(e);
                }
            }
        };

        // Join both drains now, on every exit path, so neither thread leaks.
        let stdout = stdout_handle.and_then(|h| h.join().ok()).unwrap_or_default();
        let stderr = stderr_handle.and_then(|h| h.join().ok()).unwrap_or_default();

        match outcome {
            Outcome::Cancelled => Err(DownloadError::Cancelled),
            Outcome::Stalled => Err(DownloadError::ExtractFailed(format!(
                "download stalled: no data received for {}s — check your connection and retry",
                STALL_TIMEOUT.as_secs()
            ))),
            Outcome::WaitErr(e) => Err(DownloadError::ExtractFailed(format!("wait go-pmtiles: {e}"))),
            Outcome::Exited(status) => {
                if status.success() {
                    Ok(())
                } else {
                    Err(DownloadError::ExtractFailed(sidecar_exit_error(
                        status.code(),
                        &stderr,
                        &stdout,
                    )))
                }
            }
        }
    }
}

/// Emit the terminal `basemap:download-done` event. Every exit path of
/// `download_pack_blocking` — success, install error, AND the early returns
/// (build_request failure, duplicate-in-flight reject) — emits exactly one of
/// these so the UI's `useDownloadProgress` always clears its row (a missing
/// done-event leaves a phantom in-progress row that only a restart removes).
fn emit_done(app: &AppHandle, pack_id: &str, ok: bool, error: Option<String>) {
    let _ = app.emit(
        DONE_EVENT,
        &DownloadDone {
            pack_id: pack_id.to_string(),
            ok,
            error,
        },
    );
}

/// Format a non-zero-exit error for the go-pmtiles sidecar, combining whatever it
/// wrote to stderr AND stdout. go-pmtiles emits its real failure (e.g. "Failed to
/// create range reader ... HTTP error: 404" when the planet build URL has rotated)
/// to STDOUT, so an stderr-only message was empty + useless. Pure so the formatting
/// is unit-tested without spawning a process.
fn sidecar_exit_error(code: Option<i32>, stderr: &str, stdout: &str) -> String {
    let detail = [stderr, stdout]
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" | ");
    format!("go-pmtiles exit {code:?}: {detail}")
}

/// Resolve the bundled go-pmtiles sidecar. Tauri places an `externalBin` next to
/// the main executable with the target-triple suffix stripped (`pmtiles`). In a
/// dev run (no bundle) fall back to `pmtiles` on `PATH` so the feature is
/// exercisable from `tauri dev`.
fn resolve_sidecar() -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let sibling = dir.join("pmtiles");
            if sibling.exists() {
                return sibling;
            }
        }
    }
    PathBuf::from("pmtiles")
}

/// Free bytes on the filesystem holding `path`, or `None` if free space cannot be
/// determined (statvfs failed — e.g. the path does not exist). The caller MUST
/// treat `None` as "do not proceed": a download that cannot confirm free space
/// must not silently wave a multi-GB transfer onto an unknown filesystem. Kept
/// distinct from `Some(0)` (a real, determinable, full filesystem) so the command
/// layer can surface a stat failure ("could not determine free space") separately
/// from a genuine out-of-space rejection (`InsufficientSpace`) — Codex #2. (The
/// prior contract returned a fail-closed `0`, which conflated the two and could
/// report "0 available" when the dir was merely missing.)
fn available_bytes(path: &Path) -> Option<u64> {
    match nix::sys::statvfs::statvfs(path) {
        Ok(s) => {
            // On the 64-bit Linux build targets both are u64; the explicit bindings
            // avoid an `as` cast (clippy unnecessary_cast under -D warnings).
            let blocks: u64 = s.blocks_available();
            let frag: u64 = s.fragment_size();
            Some(blocks.saturating_mul(frag))
        }
        Err(_) => None,
    }
}

/// What the webview sends to download a pack: a preset tier centered on the
/// operator grid centroid (full detail), or a named continent at a chosen detail
/// tier. The continent carries `tier_id` (tuxlink-8g28) so the backend can apply
/// that tier's `maxzoom` to the continent-scale bbox — without it, a continent
/// always extracted at full z14 (a 17–30 GB runaway that never finished).
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum DownloadArgs {
    Tier { tier_id: String, lon0: f64, lat0: f64 },
    Continent { continent_id: String, tier_id: String },
}

/// The pack list + total disk used (for the manager's disk-used display).
#[derive(Debug, Clone, Serialize)]
pub struct PacksList {
    pub packs: Vec<InstalledPack>,
    pub total_bytes: u64,
}

/// `basemap_download_pack` result: the installed pack plus whether it is live
/// immediately. `requires_restart` is true only when the pack installed durably
/// (validated, renamed into place, manifest written) but the in-memory
/// `PmtilesRegistry` registration failed — `init_packs` re-registers every
/// installed pack on the next startup, so the pack is usable then. The UI uses
/// this to (a) surface an honest "installed — restart to use offline" notice and
/// (b) NOT signal the live map to add a `tile://pmtiles/<id>` source the registry
/// cannot serve yet (Codex #5). `#[serde(flatten)]` keeps `InstalledPack`'s own
/// (snake_case) field names so the existing TS `InstalledPack` shape is preserved;
/// only the added flag is camelCased.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadResult {
    #[serde(flatten)]
    pub pack: InstalledPack,
    pub requires_restart: bool,
}

/// The cached manifest (bundled default until a refresh succeeds).
#[tauri::command]
pub fn basemap_get_manifest(state: State<'_, Arc<BasemapState>>) -> RegionManifest {
    state.manifest.read().expect("manifest lock").clone()
}

/// Refresh the cached manifest from the canonical raw URL (Rust egress; CSP
/// closed) and store it in `state`. On any failure the cached manifest is left
/// untouched and the error is returned — every caller keeps working with the
/// previous/bundled manifest. Shared by the explicit `basemap_refresh_manifest`
/// command and the best-effort pre-download refresh in `basemap_download_pack`
/// (Codex #1, race-free freshness).
async fn refresh_manifest_into(state: &BasemapState) -> Result<RegionManifest, String> {
    // Bounded fetch: the bytes are re-validated by parse(), but a timeout keeps a
    // slow/hung endpoint from stalling the command. (The host is pinned by
    // validate_planet_url on the payload, not by the transport; redirect-following
    // inside reqwest/go-pmtiles is out of scope — see region_manifest SECURITY.)
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| format!("manifest client: {e}"))?;
    let body = client
        .get(MANIFEST_URL)
        .send()
        .await
        .map_err(|e| format!("fetch manifest: {e}"))?
        .text()
        .await
        .map_err(|e| format!("read manifest: {e}"))?;
    let manifest = RegionManifest::parse(&body).map_err(|e| e.to_string())?;
    *state.manifest.write().expect("manifest lock") = manifest.clone();
    Ok(manifest)
}

/// Refresh the manifest from the canonical raw URL (Rust egress; CSP closed). On
/// any failure the cached manifest is kept and an error is returned — the UI
/// keeps working with the previous/bundled manifest.
#[tauri::command]
pub async fn basemap_refresh_manifest(
    state: State<'_, Arc<BasemapState>>,
) -> Result<RegionManifest, String> {
    refresh_manifest_into(state.inner()).await
}

/// List installed packs + total disk used.
#[tauri::command]
pub fn basemap_list_packs(state: State<'_, Arc<BasemapState>>) -> PacksList {
    let m = download::load_manifest(&state.packs_dir);
    PacksList {
        total_bytes: m.total_bytes(),
        packs: m.packs,
    }
}

/// Download + validate + install a region pack, then register it so
/// `tile://pmtiles/<id>` serves it.
///
/// tuxlink-mgus: this is **async** and runs the blocking work (a multi-GB sidecar
/// extract + fs install) inside `spawn_blocking`, matching the project's idiom
/// (lib.rs / winlink_backend.rs). A SYNC Tauri command runs on the MAIN thread —
/// so the previous sync version pinned the UI for the entire download (a continent
/// at z14 is many GB → an unrecoverable freeze) AND starved the `download-progress`
/// events, which are emitted from this same thread and can't reach the webview
/// while it's blocked. Off the main thread, the UI stays responsive and progress
/// flows.
#[tauri::command]
pub async fn basemap_download_pack(
    app: AppHandle,
    registry: State<'_, Arc<PmtilesRegistry>>,
    state: State<'_, Arc<BasemapState>>,
    args: DownloadArgs,
) -> Result<DownloadResult, String> {
    // `State` borrows the invocation and can't cross the spawn_blocking boundary;
    // clone the inner Arcs (cheap, Send + 'static) and move them in.
    let registry = registry.inner().clone();
    let state = state.inner().clone();

    // Codex #1 (HIGH): refresh the cached manifest best-effort HERE, in the async
    // command body, BEFORE the blocking work reads `state.manifest` to build the
    // request. Awaiting completes the refresh (and its write to `state.manifest`)
    // before `spawn_blocking` runs `build_request`, so a download can never build
    // from a stale `planet_url` — which is what let a rotated Protomaps build 404
    // the extract. This replaces relying on the UI's fire-and-forget on-open
    // refresh (which a quick click could outrun). A refresh failure (e.g. offline)
    // is non-fatal: keep the cached/bundled manifest and proceed.
    if let Err(e) = refresh_manifest_into(&state).await {
        tracing::warn!(
            target: "tuxlink::basemap",
            error = %e,
            "pre-download manifest refresh failed; using cached manifest"
        );
    }

    tokio::task::spawn_blocking(move || download_pack_blocking(app, registry, state, args))
        .await
        .map_err(|e| format!("download task failed to run: {e}"))?
}

/// The blocking download body, run on the async worker pool (never the main
/// thread) by [`basemap_download_pack`]. Owns its `Arc`s + `AppHandle` so it is
/// `Send + 'static`. The cancel registry + progress emitter work unchanged from a
/// worker thread (`AppHandle::emit` is thread-safe; the webview is responsive).
fn download_pack_blocking(
    app: AppHandle,
    registry: Arc<PmtilesRegistry>,
    state: Arc<BasemapState>,
    args: DownloadArgs,
) -> Result<DownloadResult, String> {
    let manifest = state.manifest.read().expect("manifest lock").clone();
    let req = match build_request(&manifest, &args) {
        Ok(req) => req,
        Err(e) => {
            // C3: emit a terminal done so the UI clears the in-progress row even on
            // this pre-extract failure. The done handler latches onto whatever id
            // arrives (no progress event preceded it), so a best-effort id from the
            // args is sufficient to match the row.
            let msg = e.to_string();
            emit_done(&app, &pack_id_for_args(&args), false, Some(msg.clone()));
            return Err(msg);
        }
    };
    let extractor = SidecarExtractor {
        bin: resolve_sidecar(),
    };

    // Register a fresh cancel flag for this pack so `basemap_cancel_download` can
    // stop the in-flight extract. Removed below regardless of outcome. Reject a
    // duplicate in-flight download for the same id: overwriting the flag would
    // orphan the running sidecar's Arc, making the original uncancellable (Codex
    // review 2026-06-13, P2 — defense-in-depth; the UI also disables this).
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut cancels = state
            .download_cancels
            .lock()
            .expect("download_cancels lock poisoned");
        if cancels.contains_key(&req.id) {
            // Codex #4: do NOT emit a terminal done here. `req.id` belongs to the
            // ALREADY-RUNNING download; a `download-done` carrying it would flip
            // that live download's progress row to error (`useDownloadProgress`
            // matches done events by pack id). The duplicate caller learns of the
            // reject from THIS command's rejected promise, which clears its own row
            // — so no event is needed, and emitting one only poisons the original.
            return Err(format!("a download for {} is already in progress", req.id));
        }
        cancels.insert(req.id.clone(), cancel.clone());
    }

    // Throttled progress emitter: the poll loop samples every POLL_INTERVAL, but
    // the UI only needs ~1 repaint per EMIT_THROTTLE. `total` is the manifest's
    // typical_bytes estimate (the bar denominator).
    let pack_id = req.id.clone();
    let total = req.typical_bytes;
    let progress_app = app.clone();
    let last_emit = Mutex::new(None::<Instant>);
    let on_progress = move |bytes: u64| {
        let mut last = last_emit.lock().expect("last_emit lock poisoned");
        if download::should_emit(*last, Instant::now(), EMIT_THROTTLE) {
            *last = Some(Instant::now());
            let _ = progress_app.emit(
                PROGRESS_EVENT,
                &DownloadProgress {
                    pack_id: pack_id.clone(),
                    bytes,
                    total,
                },
            );
        }
    };

    // Serialize the install (free-space + extract + validate + atomic install +
    // manifest RMW) against any concurrent download/delete so the packs-manifest
    // read-modify-write can't lose an update (which would orphan a completed pack).
    //
    // Codex #2: ensure the packs dir exists and sample free space INSIDE the lock,
    // immediately before install_pack. Sampling under the lock means a second
    // (serialized) download measures the disk AFTER the first one installed, not
    // from a shared pre-lock snapshot both could pass. Creating the dir first means
    // a transient missing dir surfaces as a real mkdir error and a statvfs failure
    // on the now-existing dir surfaces as a distinct "could not determine free
    // space" — neither is silently conflated with a genuine out-of-space rejection.
    let result = {
        let _install = state.install_lock.lock().expect("install lock poisoned");
        match std::fs::create_dir_all(&state.packs_dir) {
            Err(e) => Err(DownloadError::Io(format!(
                "create packs dir {}: {e}",
                state.packs_dir.display()
            ))),
            Ok(()) => match available_bytes(&state.packs_dir) {
                None => Err(DownloadError::Io(format!(
                    "could not determine free space on {}",
                    state.packs_dir.display()
                ))),
                Some(available) => download::install_pack(
                    &extractor,
                    &state.packs_dir,
                    available,
                    &req,
                    &cancel,
                    &on_progress,
                ),
            },
        }
    };

    // Drop this download's cancel flag (success/err/cancel all land here).
    state
        .download_cancels
        .lock()
        .expect("download_cancels lock poisoned")
        .remove(&req.id);

    match result {
        Ok(entry) => {
            // The download durably SUCCEEDED here: the archive is renamed into place
            // and the packs-manifest entry is written (install_pack returned Ok). A
            // subsequent in-memory registry registration is the only thing that can
            // still fail, and that failure is NON-FATAL: the pack is valid on disk and
            // `init_packs` re-registers every installed pack on the next startup.
            // Throwing away a multi-GB validated download for a transient registration
            // error would be wrong — log it and report success.
            //
            // Codex #5: track whether the live registration succeeded. On failure the
            // pack is installed but `tile://pmtiles/<id>` cannot serve it until the
            // next restart re-registers it, so we report `requires_restart` to the UI
            // — which surfaces an honest "restart to use" notice and does NOT signal
            // the live map to add a source the registry can't yet serve.
            let requires_restart = if let Err(e) = registry
                .register_path(&entry.id, &download::pack_path(&state.packs_dir, &entry.id))
            {
                tracing::warn!(
                    target: "tuxlink::basemap",
                    id = %entry.id,
                    error = %e,
                    "pack installed but live registration failed; it will be re-registered on next startup"
                );
                true
            } else {
                false
            };
            let _ = app.emit(
                DONE_EVENT,
                &DownloadDone { pack_id: req.id.clone(), ok: true, error: None },
            );
            Ok(DownloadResult { pack: entry, requires_restart })
        }
        Err(e) => {
            let msg = e.to_string();
            let _ = app.emit(
                DONE_EVENT,
                &DownloadDone { pack_id: req.id.clone(), ok: false, error: Some(msg.clone()) },
            );
            Err(msg)
        }
    }
}

/// Cancel an in-flight pack download. Sets the cancel flag for `pack_id`; the
/// running extract's poll loop sees it, kills the sidecar, and unwinds with
/// `Cancelled` (the atomic-install cleanup guard removes the `.part`, so no
/// installed pack persists). A no-op if no download for that id is in flight.
#[tauri::command]
pub fn basemap_cancel_download(state: State<'_, Arc<BasemapState>>, pack_id: String) {
    if let Some(flag) = state
        .download_cancels
        .lock()
        .expect("download_cancels lock poisoned")
        .get(&pack_id)
    {
        flag.store(true, Ordering::SeqCst);
    }
}

/// Delete a pack: archive + manifest entry + registry. Returns true if present.
#[tauri::command]
pub fn basemap_delete_pack(
    registry: State<'_, Arc<PmtilesRegistry>>,
    state: State<'_, Arc<BasemapState>>,
    id: String,
) -> Result<bool, String> {
    // Same lock as install: delete also does a manifest read-modify-write.
    let _install = state.install_lock.lock().expect("install lock poisoned");
    let removed = download::delete_pack(&state.packs_dir, &id).map_err(|e| e.to_string())?;
    registry.remove(&id);
    Ok(removed)
}

/// Bytes the free-space pre-flight must reserve for a pack whose estimate is
/// `typical`. C7: validation accepts a downloaded archive up to `typical *
/// BUDGET_MULT`, so the disk gate must reserve at least that much — otherwise an
/// archive between 1.2x and 3x the estimate passes validation having never been
/// gated against free space, and the install fails on a full disk after a multi-GB
/// transfer. Reserve `max(typical * 6/5, typical * BUDGET_MULT)` so the pre-flight
/// covers the entire size envelope validation will accept (BUDGET_MULT ≥ 6/5, so
/// this is the budget; the `max` documents the intent and stays correct if the
/// constants change). Saturating so a huge estimate cannot overflow.
fn needed_bytes_for(typical: u64) -> u64 {
    let margin = typical.saturating_mul(NEEDED_MARGIN_NUM) / NEEDED_MARGIN_DEN;
    let budget = typical.saturating_mul(BUDGET_MULT);
    margin.max(budget)
}

/// Estimated bytes for a continent-scale extract clipped to `maxzoom`, given the
/// manifest's z14 `baseline_z14` (the `Continent.typical_bytes`). Each zoom below
/// z14 divides the estimate by [`CONTINENT_ZOOM_SHRINK`] (ceil-div, so it biases
/// HIGH and never rounds to 0). A `maxzoom >= PACK_MAXZOOM` returns the baseline
/// unchanged. Saturating throughout. Pure → unit-tested (tuxlink-8g28).
///
/// Why this matters: the free-space gate reserves `typical_bytes * 3` and `validate`
/// rejects above `typical_bytes * 3`. Reusing the flat z14 baseline (30 GB) for a
/// shallow (z8, ~hundreds of MB) extract would demand 90 GB free — rejecting on
/// exactly the disks where an operator picks low detail *because* space is tight.
fn continent_estimate(baseline_z14: u64, maxzoom: u8) -> u64 {
    let levels_below = PACK_MAXZOOM.saturating_sub(maxzoom) as u32;
    let divisor = CONTINENT_ZOOM_SHRINK.saturating_pow(levels_below).max(1);
    // ceil-div biases high; `.max(1)` so an extreme shrink never yields 0 (which
    // would make needed_bytes / size_budget degenerate).
    (baseline_z14.saturating_add(divisor - 1) / divisor).max(1)
}

/// Build a validated [`PackRequest`] from the manifest + the webview's args.
fn build_request(manifest: &RegionManifest, args: &DownloadArgs) -> Result<PackRequest, DownloadError> {
    let now = chrono::Utc::now().to_rfc3339();
    match args {
        DownloadArgs::Tier { tier_id, lon0, lat0 } => {
            let tier = manifest
                .tiers
                .iter()
                .find(|t| &t.id == tier_id)
                .ok_or_else(|| DownloadError::ExtractFailed(format!("unknown tier {tier_id:?}")))?;
            let bbox = packs::tier_bbox(*lon0, *lat0, tier.half_deg[0], tier.half_deg[1])
                .map_err(|e| DownloadError::ExtractFailed(e.to_string()))?;
            Ok(PackRequest {
                id: packs::tier_pack_id(tier_id, *lon0, *lat0),
                label: format!("{} — {}", tier.label, grid_label(*lon0, *lat0)),
                planet_url: manifest.planet_url.clone(),
                bbox,
                maxzoom: PACK_MAXZOOM,
                source_build: manifest.planet_build.clone(),
                typical_bytes: tier.typical_bytes,
                needed_bytes: needed_bytes_for(tier.typical_bytes),
                size_budget: tier.typical_bytes.saturating_mul(BUDGET_MULT),
                installed_at: now,
            })
        }
        DownloadArgs::Continent { continent_id, tier_id } => {
            let c = manifest
                .continents
                .iter()
                .find(|c| &c.id == continent_id)
                .ok_or_else(|| {
                    DownloadError::ExtractFailed(format!("unknown continent {continent_id:?}"))
                })?;
            // tuxlink-8g28: the chosen detail tier supplies the maxzoom for the
            // continent-scale bbox (the size lever at continent scale), replacing the
            // flat PACK_MAXZOOM that always produced a full-detail 17–30 GB runaway.
            // `tier.maxzoom` is bounded `1..=MAX_TIER_MAXZOOM` by manifest validation.
            let tier = manifest
                .tiers
                .iter()
                .find(|t| &t.id == tier_id)
                .ok_or_else(|| DownloadError::ExtractFailed(format!("unknown tier {tier_id:?}")))?;
            let bbox = packs::continent_bbox(c.bbox)
                .map_err(|e| DownloadError::ExtractFailed(e.to_string()))?;
            // Size estimate scales with the tier's maxzoom so the free-space gate +
            // validation budget + progress denominator track the actual (smaller)
            // shallow-detail extract rather than the flat z14 baseline.
            let estimate = continent_estimate(c.typical_bytes, tier.maxzoom);
            Ok(PackRequest {
                id: packs::continent_pack_id(continent_id),
                label: format!("{} — {}", c.label, tier.label),
                planet_url: manifest.planet_url.clone(),
                bbox,
                maxzoom: tier.maxzoom,
                source_build: manifest.planet_build.clone(),
                typical_bytes: estimate,
                needed_bytes: needed_bytes_for(estimate),
                size_budget: estimate.saturating_mul(BUDGET_MULT),
                installed_at: now,
            })
        }
    }
}

/// Best-effort pack id derived directly from the download args, for the C3
/// done-event on a pre-`build_request` failure (when no validated `PackRequest`
/// exists yet). Mirrors the id `build_request` would compute, so the emitted done
/// event carries a meaningful, matchable id. The UI's done handler latches onto
/// whatever id arrives when no progress event preceded it, so any stable id clears
/// the row; this keeps it consistent with the success path.
fn pack_id_for_args(args: &DownloadArgs) -> String {
    match args {
        DownloadArgs::Tier { tier_id, lon0, lat0 } => packs::tier_pack_id(tier_id, *lon0, *lat0),
        // The continent pack id is detail-independent (one pack per continent;
        // re-downloading at a different detail tier overwrites it), so `tier_id`
        // is not part of the id.
        DownloadArgs::Continent { continent_id, .. } => packs::continent_pack_id(continent_id),
    }
}

/// Short human label for a centroid (e.g. `33.5,-112.0`) for the pack list.
fn grid_label(lon0: f64, lat0: f64) -> String {
    format!("{lat0:.1},{lon0:.1}")
}

/// Startup wiring: resolve the packs dir, sweep interrupted/orphaned files, and
/// re-register every installed pack so `tile://pmtiles/<id>` resolves after a
/// restart. Called from `lib.rs` `.setup()` once `app_data_dir` is known. Returns
/// the `BasemapState` to manage. Best-effort: a sweep/register failure is logged,
/// never fatal.
pub fn init_packs(packs_dir: PathBuf, registry: &PmtilesRegistry) -> BasemapState {
    // Ensure the packs dir exists at startup so the FIRST download's free-space
    // pre-flight (`available_bytes` → statvfs) stats a real directory instead of a
    // missing path. A missing path now fails CLOSED (returns 0), which would reject
    // the very first download until a restart created the dir as a side effect of
    // install_pack; creating it here makes the gate measure the actual filesystem.
    // Best-effort: a failure is logged, never fatal (install_pack also creates it).
    if let Err(e) = std::fs::create_dir_all(&packs_dir) {
        tracing::warn!(target: "basemap", error = %e, "failed to pre-create packs dir");
    }
    let manifest = download::load_manifest(&packs_dir);
    let swept = download::sweep_orphans(&packs_dir, &manifest);
    if swept > 0 {
        tracing::info!(target: "basemap", swept, "swept orphaned pack files");
    }
    for p in &manifest.packs {
        let path = download::pack_path(&packs_dir, &p.id);
        match registry.register_path(&p.id, &path) {
            Ok(len) => tracing::info!(target: "basemap", id = %p.id, bytes = len, "registered installed pack"),
            Err(e) => tracing::warn!(target: "basemap", id = %p.id, error = %e, "failed to register installed pack"),
        }
    }
    BasemapState::new(packs_dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn download_args_deserialize_tier_and_continent() {
        let tier: DownloadArgs =
            serde_json::from_str(r#"{"kind":"tier","tier_id":"wide","lon0":-112.0,"lat0":33.5}"#).unwrap();
        assert!(matches!(tier, DownloadArgs::Tier { .. }));
        let cont: DownloadArgs =
            serde_json::from_str(r#"{"kind":"continent","continent_id":"na","tier_id":"local"}"#).unwrap();
        assert!(matches!(cont, DownloadArgs::Continent { .. }));
    }

    #[test]
    fn build_request_for_tier_uses_manifest_url_and_app_maxzoom() {
        let m = RegionManifest::bundled_default();
        let req = build_request(
            &m,
            &DownloadArgs::Tier {
                tier_id: "wide".into(),
                lon0: -112.0,
                lat0: 33.5,
            },
        )
        .unwrap();
        assert_eq!(req.planet_url, m.planet_url);
        assert_eq!(req.maxzoom, PACK_MAXZOOM);
        assert_eq!(req.source_build, m.planet_build);
        assert!(packs::is_safe_pack_id(&req.id));
        // C7: the free-space pre-flight must reserve at least the validation budget,
        // so needed_bytes covers the full envelope validation accepts (they're equal
        // here because BUDGET_MULT ≥ the 6/5 margin).
        assert!(req.needed_bytes >= req.size_budget);
    }

    #[test]
    fn build_request_rejects_unknown_tier() {
        let m = RegionManifest::bundled_default();
        let err = build_request(
            &m,
            &DownloadArgs::Tier {
                tier_id: "nope".into(),
                lon0: 0.0,
                lat0: 0.0,
            },
        );
        assert!(err.is_err());
    }

    #[test]
    fn build_request_for_continent() {
        let m = RegionManifest::bundled_default();
        let req = build_request(
            &m,
            &DownloadArgs::Continent { continent_id: "na".into(), tier_id: "local".into() },
        )
        .unwrap();
        assert_eq!(req.id, "continent-na");
        assert!(packs::is_safe_pack_id(&req.id));
        let local = m.tiers.iter().find(|t| t.id == "local").unwrap();
        let na = m.continents.iter().find(|c| c.id == "na").unwrap();
        // tuxlink-8g28: the chosen detail tier drives the continent extract's maxzoom
        // (was the flat full-detail PACK_MAXZOOM that produced the 17–30 GB runaway).
        assert_eq!(req.maxzoom, local.maxzoom);
        assert!(req.maxzoom < PACK_MAXZOOM, "low-detail continent must be shallower than z14");
        // The size estimate scales DOWN with the shallower maxzoom so the free-space
        // gate doesn't demand the full z14 baseline for a small extract.
        assert!(req.typical_bytes < na.typical_bytes);
        assert_eq!(req.typical_bytes, continent_estimate(na.typical_bytes, local.maxzoom));
        // The label carries the detail tier so the installed-pack list is honest.
        assert!(req.label.contains(&na.label) && req.label.contains(&local.label));
        // C7 invariant preserved on the continent path too.
        assert!(req.needed_bytes >= req.size_budget);
    }

    #[test]
    fn build_request_continent_wide_is_deeper_and_larger_than_local() {
        // Picking a bigger detail tier means a deeper (larger) continent extract.
        let m = RegionManifest::bundled_default();
        let local = build_request(
            &m,
            &DownloadArgs::Continent { continent_id: "na".into(), tier_id: "local".into() },
        )
        .unwrap();
        let wide = build_request(
            &m,
            &DownloadArgs::Continent { continent_id: "na".into(), tier_id: "wide".into() },
        )
        .unwrap();
        assert!(wide.maxzoom > local.maxzoom);
        assert!(wide.typical_bytes > local.typical_bytes);
    }

    #[test]
    fn build_request_rejects_unknown_continent_tier() {
        let m = RegionManifest::bundled_default();
        let err = build_request(
            &m,
            &DownloadArgs::Continent { continent_id: "na".into(), tier_id: "nope".into() },
        );
        assert!(err.is_err());
    }

    #[test]
    fn continent_estimate_scales_with_maxzoom() {
        let baseline = 30_000_000_000u64; // ~30 GB z14 continent
        // z14 (== PACK_MAXZOOM) and any maxzoom at/above it return the baseline.
        assert_eq!(continent_estimate(baseline, PACK_MAXZOOM), baseline);
        assert_eq!(continent_estimate(baseline, 20), baseline);
        // Each zoom below z14 shrinks by CONTINENT_ZOOM_SHRINK (ceil-div).
        assert_eq!(continent_estimate(baseline, 13), baseline.div_ceil(CONTINENT_ZOOM_SHRINK));
        assert_eq!(
            continent_estimate(baseline, 11),
            baseline.div_ceil(CONTINENT_ZOOM_SHRINK.pow(3))
        );
        // Strictly decreasing as detail drops, and never zero even at extreme shrink.
        assert!(continent_estimate(baseline, 8) < continent_estimate(baseline, 11));
        assert!(continent_estimate(baseline, 1) >= 1);
        assert!(continent_estimate(0, 8) >= 1);
    }

    // ── C1(a): the free-space gate must FAIL CLOSED on a stat error ──────────────

    #[test]
    fn available_bytes_returns_none_on_nonexistent_path() {
        // A path that cannot be statvfs'd (does not exist) returns None, NOT a
        // bogus large figure — the caller must refuse the download rather than wave
        // a multi-GB transfer onto a filesystem it can't measure. None is distinct
        // from Some(0) so the command surfaces "could not determine free space"
        // separately from a genuine out-of-space rejection (Codex #2).
        let missing = Path::new("/nonexistent/tuxlink/basemap/packs/definitely-not-here");
        assert_eq!(available_bytes(missing), None);
    }

    #[test]
    fn available_bytes_reports_nonzero_for_real_dir() {
        // Sanity: a real, statvfs-able directory yields Some(positive free space).
        let dir = tempfile::tempdir().unwrap();
        assert!(available_bytes(dir.path()).is_some_and(|n| n > 0));
    }

    // ── B2: the sidecar error message must surface stdout AND stderr ─────────────

    #[test]
    fn sidecar_exit_error_includes_stdout_detail() {
        // go-pmtiles writes its real failure to stdout; with stderr empty the
        // message must still carry the stdout detail (the original bug showed an
        // empty "go-pmtiles exit Some(1): ").
        let msg = sidecar_exit_error(
            Some(1),
            "",
            "Failed to create range reader ... HTTP error: 404",
        );
        assert!(msg.contains("Failed to create range reader"), "got: {msg}");
        assert!(msg.contains("Some(1)"), "got: {msg}");
    }

    #[test]
    fn sidecar_exit_error_joins_both_streams() {
        let msg = sidecar_exit_error(Some(2), "  stderr line  ", "  stdout line  ");
        // Both present, trimmed, joined by " | ".
        assert_eq!(msg, "go-pmtiles exit Some(2): stderr line | stdout line");
    }

    #[test]
    fn sidecar_exit_error_omits_empty_streams() {
        // Only stderr present → no leading/trailing separator noise.
        let msg = sidecar_exit_error(None, "boom", "");
        assert_eq!(msg, "go-pmtiles exit None: boom");
    }

    // ── C7: the free-space reservation must cover the validation budget ──────────

    #[test]
    fn needed_bytes_covers_validation_budget() {
        // An archive up to typical * BUDGET_MULT passes validation, so the disk
        // pre-flight must reserve at least that much.
        let typical = 1_000_000_000u64;
        let needed = needed_bytes_for(typical);
        assert_eq!(needed, typical.saturating_mul(BUDGET_MULT));
        assert!(needed >= typical.saturating_mul(NEEDED_MARGIN_NUM) / NEEDED_MARGIN_DEN);
    }

    #[test]
    fn build_request_needed_bytes_covers_size_budget() {
        // End-to-end through build_request: the reservation is >= the budget the
        // downloaded archive is validated against (was 1.2x < 3x budget before C7).
        let m = RegionManifest::bundled_default();
        let req = build_request(
            &m,
            &DownloadArgs::Tier { tier_id: "wide".into(), lon0: -112.0, lat0: 33.5 },
        )
        .unwrap();
        assert!(
            req.needed_bytes >= req.size_budget,
            "needed {} must cover budget {}",
            req.needed_bytes,
            req.size_budget
        );
    }

    #[test]
    fn pack_id_for_args_matches_build_request_id() {
        // The C3 best-effort id (used on a pre-build_request failure) must match the
        // id build_request would have produced, so the done event clears the row.
        let m = RegionManifest::bundled_default();
        let tier_args = DownloadArgs::Tier { tier_id: "wide".into(), lon0: -112.0, lat0: 33.5 };
        let req = build_request(&m, &tier_args).unwrap();
        assert_eq!(pack_id_for_args(&tier_args), req.id);

        let cont_args = DownloadArgs::Continent { continent_id: "na".into() };
        let creq = build_request(&m, &cont_args).unwrap();
        assert_eq!(pack_id_for_args(&cont_args), creq.id);
    }
}
