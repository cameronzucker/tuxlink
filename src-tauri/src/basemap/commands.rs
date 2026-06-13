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

use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

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
}

impl BasemapState {
    pub fn new(packs_dir: PathBuf) -> Self {
        Self {
            manifest: RwLock::new(RegionManifest::bundled_default()),
            packs_dir,
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
    ) -> Result<(), DownloadError> {
        // No shell — argv tokens are passed directly to execvp. planet_url/bbox
        // are pre-validated; maxzoom/dest are app-controlled.
        let output = std::process::Command::new(&self.bin)
            .arg("extract")
            .arg(planet_url)
            .arg(dest)
            .arg(format!("--maxzoom={maxzoom}"))
            .arg(format!("--bbox={}", bbox.to_arg()))
            .output()
            .map_err(|e| DownloadError::ExtractFailed(format!("spawn go-pmtiles: {e}")))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(DownloadError::ExtractFailed(format!(
                "go-pmtiles exit {:?}: {}",
                output.status.code(),
                stderr.trim()
            )));
        }
        Ok(())
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
        Ok(s) => (s.blocks_available() as u64).saturating_mul(s.fragment_size()),
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
    let body = reqwest::get(MANIFEST_URL)
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
    let entry = download::install_pack(&extractor, &state.packs_dir, available, &req)
        .map_err(|e| e.to_string())?;
    // Register so the new pack is served immediately (no restart needed).
    registry
        .register_path(&entry.id, &download::pack_path(&state.packs_dir, &entry.id))
        .map_err(|e| format!("register pack: {e}"))?;
    Ok(entry)
}

/// Delete a pack: archive + manifest entry + registry. Returns true if present.
#[tauri::command]
pub fn basemap_delete_pack(
    registry: State<'_, Arc<PmtilesRegistry>>,
    state: State<'_, Arc<BasemapState>>,
    id: String,
) -> Result<bool, String> {
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
