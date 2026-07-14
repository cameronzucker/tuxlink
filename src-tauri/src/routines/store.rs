//! `DefinitionStore`: one `<routine>.json` file per routine under
//! `config_path().parent()/routines/`, plus a sidecar `enabled.json` set.
//!
//! Definitions stay portable (spec §14) — the enabled flag lives ONLY in the
//! sidecar set, never in the definition file on disk, so a routine exported
//! from one station and imported on another carries no local on/off state.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use tuxlink_routines::engine::RoutineLookup;
use tuxlink_routines::error::RoutineParseError;
use tuxlink_routines::types::{RoutineDef, TransmitMode, Trigger};

use super::atomic_write;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("routine JSON is invalid: {0}")]
    Parse(#[from] RoutineParseError),
    #[error("json serialize failed: {0}")]
    Serialize(#[from] serde_json::Error),
    #[error("routine not found: {0}")]
    NotFound(String),
    #[error(
        "routine name {0:?} is invalid — kebab-case, starts a-z, chars [a-z0-9-], length 1-64"
    )]
    InvalidName(String),
}

/// Validates a routine name BEFORE it is ever interpolated into a filesystem
/// path. Kebab-case: non-empty, ≤ 64 bytes, first char `[a-z]`, remaining
/// chars `[a-z0-9-]`. This is the single chokepoint that keeps a routine name
/// like `"../config"` or `"a/b"` from escaping the store directory and
/// clobbering an arbitrary file (e.g. the app's real `config.json`) — every
/// disk path this module builds from a caller-supplied name MUST pass through
/// here first.
fn valid_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 64 {
        return false;
    }
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_lowercase() {
        return false;
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// List-view row for the routines catalog UI (spec §14). `enabled` is
/// resolved from the sidecar set at list-time — it is never read from the
/// definition file itself.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutineSummary {
    pub routine: String,
    pub transmit_mode: TransmitMode,
    pub enabled: bool,
    pub triggers: Vec<Trigger>,
}

/// File-backed store of routine definitions. Stateless wrapper around `dir`:
/// every method reads/writes disk directly (no in-memory cache), so the
/// engine's `EngineConfig.lookup` closure and the Tauri command layer never
/// race a cache that's gone stale relative to a concurrent `save`/`delete`.
///
/// Single-writer assumption (mirrors `config::write_config_atomic`): each
/// individual `save`/`delete`/`set_enabled` call is atomic on disk, but two
/// concurrent read-modify-write cycles (e.g. two `set_enabled` calls racing
/// on the same `enabled.json` sidecar) are NOT serialized against each other
/// — the second writer can silently revert the first's change. Out of scope
/// here; the session layer's mutex (plan Task 5) is where cross-call
/// serialization lands.
pub struct DefinitionStore {
    dir: PathBuf,
}

impl DefinitionStore {
    /// Opens (creating if absent) the store directory. Directory-creation
    /// failure is swallowed here — `open`'s signature returns `Self`, not a
    /// `Result` (plan interface contract); a permissions problem surfaces on
    /// the first `save`/`list` call instead, same as a stale/missing dir would.
    pub fn open(dir: PathBuf) -> Self {
        let _ = std::fs::create_dir_all(&dir);
        Self { dir }
    }

    fn def_path(&self, name: &str) -> PathBuf {
        self.dir.join(format!("{name}.json"))
    }

    fn enabled_path(&self) -> PathBuf {
        self.dir.join("enabled.json")
    }

    fn read_enabled_set(&self) -> HashSet<String> {
        match std::fs::read_to_string(self.enabled_path()) {
            Ok(raw) => serde_json::from_str(&raw).unwrap_or_default(),
            Err(_) => HashSet::new(),
        }
    }

    fn write_enabled_set(&self, set: &HashSet<String>) -> Result<(), StoreError> {
        let mut names: Vec<&String> = set.iter().collect();
        names.sort();
        let json = serde_json::to_vec_pretty(&names)?;
        atomic_write(&self.enabled_path(), &json)?;
        Ok(())
    }

    /// All routines currently on disk, alphabetical by name. Entries that fail
    /// to parse (foreign file dropped in the dir, a future-schema export) are
    /// skipped rather than surfaced as a hard error — this is a UI listing, not
    /// a load path that must fail closed.
    pub fn list(&self) -> Vec<RoutineSummary> {
        let enabled = self.read_enabled_set();
        let mut out = Vec::new();
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return out;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.file_name().and_then(|n| n.to_str()) == Some("enabled.json") {
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let Ok(raw) = std::fs::read_to_string(&path) else {
                continue;
            };
            let Ok(def) = RoutineDef::parse(&raw) else {
                continue;
            };
            // get()/delete()/set_enabled() all key off the FILENAME stem, not
            // the body's `routine` field. A file dropped in externally (or
            // hand-edited) can disagree with its own filename — list() must
            // skip it rather than surface a summary that other store methods
            // can't actually address by the name it displays.
            let stem = path.file_stem().and_then(|s| s.to_str());
            if stem != Some(def.routine.as_str()) {
                tracing::warn!(
                    target: "tuxlink::routines",
                    file_stem = stem.unwrap_or("<non-utf8>"),
                    body_routine = %def.routine,
                    "routine definition file's name and body disagree; skipping",
                );
                continue;
            }
            out.push(RoutineSummary {
                enabled: enabled.contains(&def.routine),
                transmit_mode: def.transmit_mode,
                triggers: def.triggers,
                routine: def.routine,
            });
        }
        out.sort_by(|a, b| a.routine.cmp(&b.routine));
        out
    }

    pub fn get(&self, name: &str) -> Option<RoutineDef> {
        if !valid_name(name) {
            return None;
        }
        let raw = std::fs::read_to_string(self.def_path(name)).ok()?;
        RoutineDef::parse(&raw).ok()
    }

    /// Atomic write. Re-validates the serialized form via `RoutineDef::parse`
    /// BEFORE it ever touches disk: `RoutineDef`'s fields are all `pub`, so a
    /// caller can hand-construct one with an out-of-range value (e.g. an
    /// unsupported `schema_version`) without going through `::parse` first —
    /// this catches that case rather than persisting an unreadable file.
    ///
    /// Also rejects a non-kebab-case `def.routine` BEFORE any disk write —
    /// `def_path` interpolates the name straight into a filename, so an
    /// unvalidated `"../config"` would let a save escape the store directory
    /// and overwrite an arbitrary file (e.g. the app's real `config.json`).
    pub fn save(&self, def: &RoutineDef) -> Result<(), StoreError> {
        if !valid_name(&def.routine) {
            return Err(StoreError::InvalidName(def.routine.clone()));
        }
        let json = serde_json::to_vec_pretty(def)?;
        let json_str = std::str::from_utf8(&json).expect("serde_json output is valid UTF-8");
        RoutineDef::parse(json_str)?;
        atomic_write(&self.def_path(&def.routine), &json)?;
        Ok(())
    }

    /// Removes the definition file and, if present, its `enabled.json` entry.
    /// Errors `NotFound` if no definition file exists for `name`, or
    /// `InvalidName` if `name` isn't a valid store-managed name (checked
    /// before any path is built, same chokepoint as `save`/`get`).
    pub fn delete(&self, name: &str) -> Result<(), StoreError> {
        if !valid_name(name) {
            return Err(StoreError::InvalidName(name.to_string()));
        }
        let path = self.def_path(name);
        if !path.exists() {
            return Err(StoreError::NotFound(name.to_string()));
        }
        std::fs::remove_file(&path)?;
        let mut enabled = self.read_enabled_set();
        if enabled.remove(name) {
            self.write_enabled_set(&enabled)?;
        }
        Ok(())
    }

    /// Flip a routine's enabled bit on disk. This is the STORE half of an
    /// enable/disable, and on its own it is invisible to the RUNNING scheduler:
    /// the scheduler is woken by `RoutinesState::emit`'s
    /// `LibraryChanged{entity: routine}` ping — `emit` is the wake chokepoint —
    /// and the store does not emit. Callers who want the change to take effect
    /// promptly (rather than whenever the scheduler next re-reads the store, up
    /// to `scheduler::MAX_SLEEP_SECS` away) go through
    /// [`super::commands::set_routine_enabled`]: the one path that emits, and,
    /// on the enable side, the one path that anchors the routine's cadence at
    /// the enable instant.
    ///
    /// Calling this directly is legitimate — the scheduler's own tests do, to
    /// simulate a LOST wake ping — and nothing unsafe follows from one: the
    /// scheduler re-reads the store on every pass, and it re-checks this bit
    /// again immediately before it starts a run, so a disable is never *skipped*,
    /// only possibly *delayed*.
    pub fn set_enabled(&self, name: &str, enabled: bool) -> Result<(), StoreError> {
        if !valid_name(name) {
            return Err(StoreError::InvalidName(name.to_string()));
        }
        let mut set = self.read_enabled_set();
        if enabled {
            set.insert(name.to_string());
        } else {
            set.remove(name);
        }
        self.write_enabled_set(&set)
    }

    /// Absent from the sidecar set means disabled — a saved-but-never-enabled
    /// routine never fires (fail-closed default, consistent with the project's
    /// no-added-safeguards-but-also-no-surprise-automation posture).
    pub fn is_enabled(&self, name: &str) -> bool {
        self.read_enabled_set().contains(name)
    }

    /// `EngineConfig.lookup` seam: a cheap `Fn` closure resolving a routine
    /// name to its definition, reading disk fresh on every call (same
    /// semantics as [`Self::get`], just decoupled from the store's lifetime).
    ///
    /// Same name-validation chokepoint as `get`/`save`/`delete`: an invalid
    /// name (e.g. a `"routine": "../config"` trigger reference resolved at
    /// engine runtime) resolves to `None` rather than building a path outside
    /// the store directory.
    pub fn lookup_fn(&self) -> RoutineLookup {
        let dir = self.dir.clone();
        Arc::new(move |name: &str| {
            if !valid_name(name) {
                return None;
            }
            let path = dir.join(format!("{name}.json"));
            let raw = std::fs::read_to_string(path).ok()?;
            RoutineDef::parse(&raw).ok()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tuxlink_routines::types::{OnInterrupted, SUPPORTED_SCHEMA_VERSION};

    fn minimal_def(name: &str) -> RoutineDef {
        RoutineDef {
            routine: name.to_string(),
            schema_version: SUPPORTED_SCHEMA_VERSION,
            transmit_mode: TransmitMode::Automatic,
            transmit_ack: None,
            on_interrupted: OnInterrupted::Stay,
            inputs: vec![],
            triggers: vec![Trigger::Manual],
            tracks: vec![],
        }
    }

    #[test]
    fn save_then_get_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let store = DefinitionStore::open(dir.path().join("routines"));
        let def = minimal_def("morning-ics-cycle");
        store.save(&def).unwrap();
        let loaded = store.get("morning-ics-cycle").expect("must round-trip");
        assert_eq!(loaded, def);
    }

    #[test]
    fn get_missing_routine_is_none() {
        let dir = tempfile::tempdir().unwrap();
        let store = DefinitionStore::open(dir.path().join("routines"));
        assert!(store.get("no-such-routine").is_none());
    }

    #[test]
    fn save_rejects_invalid_json_shape() {
        let dir = tempfile::tempdir().unwrap();
        let store = DefinitionStore::open(dir.path().join("routines"));
        // RoutineDef's fields are all `pub`, so a caller can hand-construct one
        // that never went through `RoutineDef::parse` — including an
        // unsupported schema_version. `save` must reject this before writing.
        let mut bad = minimal_def("bad-schema");
        bad.schema_version = 99;
        let err = store.save(&bad).unwrap_err();
        assert!(
            matches!(
                err,
                StoreError::Parse(RoutineParseError::UnsupportedSchemaVersion(99))
            ),
            "expected an UnsupportedSchemaVersion parse error, got {err:?}"
        );
        assert!(
            store.get("bad-schema").is_none(),
            "a rejected save must not have written a file"
        );
    }

    #[test]
    fn enabled_flag_survives_reopen_and_is_absent_from_definition_file() {
        let dir = tempfile::tempdir().unwrap();
        let routines_dir = dir.path().join("routines");
        let store = DefinitionStore::open(routines_dir.clone());
        let def = minimal_def("evening-check");
        store.save(&def).unwrap();

        assert!(
            !store.is_enabled("evening-check"),
            "new routines default disabled"
        );
        store.set_enabled("evening-check", true).unwrap();
        assert!(store.is_enabled("evening-check"));

        // The definition file on disk must NOT carry the enabled flag.
        let raw = std::fs::read_to_string(routines_dir.join("evening-check.json")).unwrap();
        assert!(
            !raw.to_lowercase().contains("enabled"),
            "definition file must stay portable, no enabled flag: {raw}"
        );

        // Reopening the store (fresh struct, same dir) must still see it enabled.
        let reopened = DefinitionStore::open(routines_dir);
        assert!(reopened.is_enabled("evening-check"));

        reopened.set_enabled("evening-check", false).unwrap();
        assert!(!DefinitionStore::open(dir.path().join("routines")).is_enabled("evening-check"));
    }

    #[test]
    fn delete_removes_file_and_enabled_entry() {
        let dir = tempfile::tempdir().unwrap();
        let store = DefinitionStore::open(dir.path().join("routines"));
        let def = minimal_def("one-shot");
        store.save(&def).unwrap();
        store.set_enabled("one-shot", true).unwrap();
        assert!(store.get("one-shot").is_some());

        store.delete("one-shot").unwrap();
        assert!(store.get("one-shot").is_none());
        assert!(
            !store.is_enabled("one-shot"),
            "delete must also clear the sidecar enabled entry"
        );
    }

    #[test]
    fn delete_missing_routine_errors_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let store = DefinitionStore::open(dir.path().join("routines"));
        let err = store.delete("ghost").unwrap_err();
        assert!(matches!(err, StoreError::NotFound(name) if name == "ghost"));
    }

    #[test]
    fn lookup_fn_resolves_saved_routines() {
        let dir = tempfile::tempdir().unwrap();
        let store = DefinitionStore::open(dir.path().join("routines"));
        let def = minimal_def("lookup-target");
        store.save(&def).unwrap();

        let lookup = store.lookup_fn();
        assert_eq!(lookup("lookup-target"), Some(def));
        assert_eq!(lookup("nonexistent"), None);

        // The closure keeps working after a further mutation through the store
        // (it reads disk fresh, not a snapshot taken at lookup_fn() time).
        store.delete("lookup-target").unwrap();
        assert_eq!(lookup("lookup-target"), None);
    }

    #[test]
    fn list_reflects_enabled_flag_and_sorts_alphabetically() {
        let dir = tempfile::tempdir().unwrap();
        let store = DefinitionStore::open(dir.path().join("routines"));
        store.save(&minimal_def("zeta")).unwrap();
        store.save(&minimal_def("alpha")).unwrap();
        store.set_enabled("alpha", true).unwrap();

        let summaries = store.list();
        assert_eq!(summaries.len(), 2);
        assert_eq!(summaries[0].routine, "alpha");
        assert!(summaries[0].enabled);
        assert_eq!(summaries[1].routine, "zeta");
        assert!(!summaries[1].enabled);
    }

    #[test]
    fn valid_name_accepts_kebab_case_and_rejects_traversal_and_shape_violations() {
        // Valid kebab-case names pass.
        assert!(valid_name("morning-ics-cycle"));
        assert!(valid_name("a"));
        assert!(valid_name("a1-b2-c3"));
        assert!(
            valid_name(&"a".repeat(64)),
            "64 chars is the boundary, must pass"
        );

        // Traversal / shape violations are rejected.
        assert!(!valid_name("../config"), "parent-dir traversal");
        assert!(!valid_name("a/b"), "path separator");
        assert!(!valid_name(""), "empty");
        assert!(!valid_name("UPPER"), "uppercase not allowed");
        assert!(
            !valid_name(&"a".repeat(65)),
            "65 chars exceeds the length cap"
        );
    }

    #[test]
    fn save_rejects_path_traversal_name_before_any_disk_write() {
        let dir = tempfile::tempdir().unwrap();
        let store = DefinitionStore::open(dir.path().join("routines"));

        // A routine body naming itself "../config" must never be allowed to
        // write outside the store directory (e.g. clobbering the app's real
        // config.json one level up).
        let def = minimal_def("../config");
        let err = store.save(&def).unwrap_err();
        let err_debug = format!("{err:?}");
        assert!(
            matches!(err, StoreError::InvalidName(name) if name == "../config"),
            "expected InvalidName, got {err_debug}"
        );

        // Nothing must have been written outside (or inside) the store dir.
        assert!(
            !dir.path().join("config.json").exists(),
            "traversal save must not have escaped the store directory"
        );
    }

    #[test]
    fn get_delete_set_enabled_lookup_fn_reject_invalid_names() {
        let dir = tempfile::tempdir().unwrap();
        let store = DefinitionStore::open(dir.path().join("routines"));

        assert!(store.get("../config").is_none());
        assert!(store.get("a/b").is_none());

        assert!(matches!(
            store.delete("../config"),
            Err(StoreError::InvalidName(name)) if name == "../config"
        ));

        assert!(matches!(
            store.set_enabled("../config", true),
            Err(StoreError::InvalidName(name)) if name == "../config"
        ));

        let lookup = store.lookup_fn();
        assert_eq!(lookup("../config"), None);
        assert_eq!(lookup("a/b"), None);
    }

    #[test]
    fn list_skips_file_whose_body_routine_name_disagrees_with_filename() {
        let dir = tempfile::tempdir().unwrap();
        let routines_dir = dir.path().join("routines");
        let store = DefinitionStore::open(routines_dir.clone());

        // A normally-saved routine, plus an externally-dropped file whose
        // filename stem doesn't match the `routine` field in its body (e.g.
        // hand-edited, or copied from another routine and renamed).
        store.save(&minimal_def("legit-routine")).unwrap();
        let mismatched = minimal_def("body-says-this-name");
        let json = serde_json::to_vec_pretty(&mismatched).unwrap();
        std::fs::write(routines_dir.join("filename-says-this-name.json"), json).unwrap();

        let summaries = store.list();
        assert_eq!(
            summaries.len(),
            1,
            "the mismatched file must be skipped, not listed under either name"
        );
        assert_eq!(summaries[0].routine, "legit-routine");
    }
}
