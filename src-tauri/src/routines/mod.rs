//! Routines monolith module — Tauri-side persistence, radio arbiter, action
//! catalog, and Tauri command surface over the `tuxlink-routines` engine crate.
//!
//! Spec: `docs/superpowers/specs/2026-07-13-routines-design.md` §6 (catalog),
//! §9 (arbiter), §14 (storage). Plan:
//! `docs/superpowers/plans/2026-07-13-routines-02-actions-arbiter-mount.md`.
//!
//! **Plan 2 Task 1:** [`store::DefinitionStore`] (one `<routine>.json` file
//! per routine, portable — no runtime state in the definition itself) and
//! [`presets::RadioPresetStore`] (the Radio Preset entity CRUD,
//! `radio-presets.json` beside `config.json`).
//!
//! **Plan 2 Task 2:** [`arbiter::RadioArbiter`] — the single-owner lease
//! over a rig, shared between the operator's interactive sessions and
//! routine-run steps (spec §9).
//!
//! **Plan 2 Task 3:** [`resolver::MonolithEntityResolver`]
//! — the production `EntityResolver` that resolves `@preset:`/
//! `@station-set:`/`@identity:`/`@template:` reference tokens against the
//! real Tauri-side stores (see `resolver.rs`'s module doc for the
//! per-kind service-seam recon). [`station_sets::StationSetStore`] is a
//! new small store this task added — no named station-set/group concept
//! existed anywhere else in the codebase (see `resolver.rs` and
//! `station_sets.rs` module docs).
//!
//! **Plan 2 Task 4a:** [`actions`] — the real action catalog's service
//! seams (`ConnectService`/`AprsService`/`ListenService`),
//! `ActionDeps`/`build_registry`, and the three radio actions
//! (`radio.connect`/`radio.listen`/`radio.aprs_send`, spec §6). See
//! `actions::radio`'s module doc for the transport-seam recon and the
//! ARDOP/VARA gateway-frequency gap Task 5 must close.
//!
//! **Plan 2 Task 4b:** [`actions::cat`] — the CAT verb
//! seam (`RigService`), and the five `rig.*` actions
//! (`rig.read_state`/`rig.validate_preset`/`rig.apply_preset`/
//! `rig.switch_vfo`/`rig.tune_atu`, spec §6). Two of the five
//! (`rig.switch_vfo`, `rig.tune_atu`) have no real `tux_rig::Rig` verb to
//! delegate to and return a verbatim, seam-naming unsupported error rather
//! than a stub or a side-path fake — see `actions::cat`'s module doc for
//! the full recon.
//!
//! **Plan 2 Task 4c (this landing):** [`actions::data`] — the `DataService`
//! seam and the four `data.*` actions (`data.spacewx_wwv`/
//! `data.spacewx_swpc`/`data.stationlist_update`/`data.read`, spec §6).
//! `data.spacewx_wwv` ports the frontend's WWV/WWVH `:18`/`:45` broadcast-
//! window scheduling to Rust (the real backend capture call has no notion
//! of the schedule) and sleeps to the window before capturing — a real,
//! schedule-aware wait, not a half-wired immediate call. `data.read`'s
//! `heard_stations` and `last_connected_gateway` sources have NO backend
//! seam to delegate to (heard-station positions live only in frontend React
//! state; the last-reached gateway is never persisted past a live session)
//! and return a documented honest-gap error rather than fake data — see
//! `actions::data`'s module doc for the full recon, including the exact
//! `catalog::stations::Gateway.frequencies_khz` seam `data.stationlist_update`
//! populates that Task 5's ARDOP/VARA gateway-frequency resolver (the gap
//! `actions::radio`'s module doc names) will eventually read. Later plan-2
//! tasks extend `actions` with `local.rs` and add the engine mount + Tauri
//! command surface.
//!
//! That other, banned six-syllable term for scripted automation never
//! appears in this module's symbols, JSON keys, or docs (spec Global
//! Constraints) — the feature is Routines.

pub mod actions;
pub mod arbiter;
pub mod presets;
pub mod resolver;
pub mod station_sets;
pub mod store;

pub use arbiter::{ArbiterError, Holder, HolderInfo, RadioArbiter, RadioLease};
pub use presets::{PresetError, RadioPreset, RadioPresetStore};
pub use resolver::MonolithEntityResolver;
pub use station_sets::{StationSet, StationSetError, StationSetStore};
pub use store::{DefinitionStore, RoutineSummary, StoreError};

use std::io::Write;
use std::path::Path;

/// Atomic single-write of `bytes` to `path`: same-directory tempfile, `fsync`,
/// `rename`-persist, then a parent-directory `fsync` for durability — the same
/// discipline as `config::write_config_atomic` (spec §14 requires every
/// routines-module file write go through this, not a bare `fs::write`).
///
/// Atomicity contract scope matches `write_config_atomic`: local POSIX FS
/// (ext4/btrfs/xfs) where the target file and the tempfile share a filesystem
/// (and BTRFS subvolume); NFS/FUSE semantics are undefined.
pub(crate) fn atomic_write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let parent = path.parent().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("{path:?} has no parent directory"),
        )
    })?;
    std::fs::create_dir_all(parent)?;
    let tmp = tempfile::NamedTempFile::new_in(parent)?;
    tmp.as_file().write_all(bytes)?;
    tmp.as_file().sync_all()?;
    tmp.persist(path).map_err(|e| e.error)?;
    // Parent-dir fsync: rename(2) is atomic but not durable until the parent
    // directory's metadata flushes (same rationale as write_config_atomic).
    let parent_dir = std::fs::File::open(parent)?;
    parent_dir.sync_all()?;
    Ok(())
}
