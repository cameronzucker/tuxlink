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

/// `--maxzoom` for every on-demand pack (z0–14, D1). App constant — never from
/// the manifest, so it can't be turned into an oversized extract.
const PACK_MAXZOOM: u8 = 14;

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
        // size for progress. stdout/stderr are captured only for the error
        // message on a non-zero exit — progress comes from polling `dest` size,
        // never from parsing sidecar output (robust + sidecar-agnostic).
        let mut child = Command::new(&self.bin)
            .arg("extract")
            .arg(planet_url)
            .arg(dest)
            .arg(format!("--maxzoom={maxzoom}"))
            .arg(format!("--bbox={}", bbox.to_arg()))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| DownloadError::ExtractFailed(format!("spawn go-pmtiles: {e}")))?;

        // Drain stderr on a thread so a chatty sidecar can't deadlock on a full
        // pipe while we poll; joined after exit for the error message.
        let stderr_handle = child.stderr.take().map(|mut s| {
            std::thread::spawn(move || {
                use std::io::Read;
                let mut buf = String::new();
                let _ = s.read_to_string(&mut buf);
                buf
            })
        });

        loop {
            if cancel.load(Ordering::SeqCst) {
                let _ = child.kill();
                let _ = child.wait();
                return Err(DownloadError::Cancelled);
            }
            // Poll the temp size for the progress bar. A missing/locked file early
            // on just yields 0 — not an error.
            let written = std::fs::metadata(dest).map(|m| m.len()).unwrap_or(0);
            on_progress(written);

            match child.try_wait() {
                Ok(Some(status)) => {
                    // Final size sample before reporting completion.
                    let written = std::fs::metadata(dest).map(|m| m.len()).unwrap_or(written);
                    on_progress(written);
                    if status.success() {
                        return Ok(());
                    }
                    let stderr = stderr_handle
                        .and_then(|h| h.join().ok())
                        .unwrap_or_default();
                    return Err(DownloadError::ExtractFailed(format!(
                        "go-pmtiles exit {:?}: {}",
                        status.code(),
                        stderr.trim()
                    )));
                }
                Ok(None) => std::thread::sleep(POLL_INTERVAL),
                Err(e) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(DownloadError::ExtractFailed(format!("wait go-pmtiles: {e}")));
                }
            }
        }
    }
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

/// Free bytes on the filesystem holding `path`. On a statvfs failure returns
/// `u64::MAX` (do not block a download on a stat error — validation + the size
/// budget still bound the result).
fn available_bytes(path: &Path) -> u64 {
    match nix::sys::statvfs::statvfs(path) {
        Ok(s) => {
            // On the 64-bit Linux build targets both are u64; the explicit bindings
            // avoid an `as` cast (clippy unnecessary_cast under -D warnings).
            let blocks: u64 = s.blocks_available();
            let frag: u64 = s.fragment_size();
            blocks.saturating_mul(frag)
        }
        Err(_) => u64::MAX,
    }
}

/// What the webview sends to download a pack: a preset tier centered on the
/// operator grid centroid, or a named continent.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum DownloadArgs {
    Tier { tier_id: String, lon0: f64, lat0: f64 },
    Continent { continent_id: String },
}

/// The pack list + total disk used (for the manager's disk-used display).
#[derive(Debug, Clone, Serialize)]
pub struct PacksList {
    pub packs: Vec<InstalledPack>,
    pub total_bytes: u64,
}

/// The cached manifest (bundled default until a refresh succeeds).
#[tauri::command]
pub fn basemap_get_manifest(state: State<'_, Arc<BasemapState>>) -> RegionManifest {
    state.manifest.read().expect("manifest lock").clone()
}

/// Refresh the manifest from the canonical raw URL (Rust egress; CSP closed). On
/// any failure the cached manifest is kept and an error is returned — the UI
/// keeps working with the previous/bundled manifest.
#[tauri::command]
pub async fn basemap_refresh_manifest(
    state: State<'_, Arc<BasemapState>>,
) -> Result<RegionManifest, String> {
    // Bounded fetch: the bytes are re-validated by parse(), but a timeout keeps a
    // slow/hung endpoint from stalling the command. (The host is pinned by
    // validate_planet_url on the payload, not by the transport; redirect-following
    // inside reqwest/go-pmtiles is out of scope — see region_manifest SECURITY.)
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
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
/// `tile://pmtiles/<id>` serves it. Blocking (sidecar + fs) — Tauri runs sync
/// commands off the main thread, so this does not stall the UI thread.
#[tauri::command]
pub fn basemap_download_pack(
    app: AppHandle,
    registry: State<'_, Arc<PmtilesRegistry>>,
    state: State<'_, Arc<BasemapState>>,
    args: DownloadArgs,
) -> Result<InstalledPack, String> {
    let manifest = state.manifest.read().expect("manifest lock").clone();
    let req = build_request(&manifest, &args).map_err(|e| e.to_string())?;
    let extractor = SidecarExtractor {
        bin: resolve_sidecar(),
    };
    let available = available_bytes(&state.packs_dir);

    // Register a fresh cancel flag for this pack so `basemap_cancel_download` can
    // stop the in-flight extract. Removed below regardless of outcome.
    let cancel = Arc::new(AtomicBool::new(false));
    state
        .download_cancels
        .lock()
        .expect("download_cancels lock poisoned")
        .insert(req.id.clone(), cancel.clone());

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
    let result = {
        let _install = state.install_lock.lock().expect("install lock poisoned");
        download::install_pack(&extractor, &state.packs_dir, available, &req, &cancel, &on_progress)
    };

    // Drop this download's cancel flag (success/err/cancel all land here).
    state
        .download_cancels
        .lock()
        .expect("download_cancels lock poisoned")
        .remove(&req.id);

    match result {
        Ok(entry) => {
            // Register so the new pack is served immediately (no restart needed).
            let reg = registry
                .register_path(&entry.id, &download::pack_path(&state.packs_dir, &entry.id))
                .map_err(|e| format!("register pack: {e}"));
            match reg {
                Ok(_) => {
                    let _ = app.emit(
                        DONE_EVENT,
                        &DownloadDone { pack_id: req.id.clone(), ok: true, error: None },
                    );
                    Ok(entry)
                }
                Err(e) => {
                    let _ = app.emit(
                        DONE_EVENT,
                        &DownloadDone { pack_id: req.id.clone(), ok: false, error: Some(e.clone()) },
                    );
                    Err(e)
                }
            }
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
                needed_bytes: tier.typical_bytes.saturating_mul(NEEDED_MARGIN_NUM) / NEEDED_MARGIN_DEN,
                size_budget: tier.typical_bytes.saturating_mul(BUDGET_MULT),
                installed_at: now,
            })
        }
        DownloadArgs::Continent { continent_id } => {
            let c = manifest
                .continents
                .iter()
                .find(|c| &c.id == continent_id)
                .ok_or_else(|| {
                    DownloadError::ExtractFailed(format!("unknown continent {continent_id:?}"))
                })?;
            let bbox = packs::continent_bbox(c.bbox)
                .map_err(|e| DownloadError::ExtractFailed(e.to_string()))?;
            Ok(PackRequest {
                id: packs::continent_pack_id(continent_id),
                label: c.label.clone(),
                planet_url: manifest.planet_url.clone(),
                bbox,
                maxzoom: PACK_MAXZOOM,
                source_build: manifest.planet_build.clone(),
                typical_bytes: c.typical_bytes,
                needed_bytes: c.typical_bytes.saturating_mul(NEEDED_MARGIN_NUM) / NEEDED_MARGIN_DEN,
                size_budget: c.typical_bytes.saturating_mul(BUDGET_MULT),
                installed_at: now,
            })
        }
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
            serde_json::from_str(r#"{"kind":"continent","continent_id":"na"}"#).unwrap();
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
        assert!(req.size_budget > req.needed_bytes);
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
        let req = build_request(&m, &DownloadArgs::Continent { continent_id: "na".into() }).unwrap();
        assert_eq!(req.id, "continent-na");
        assert!(packs::is_safe_pack_id(&req.id));
    }
}
