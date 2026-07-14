//! `StationSet` entity: a named, ordered list of station callsigns, CRUD over
//! a single `station-sets.json` file that lives beside `config.json` (spec
//! §14 storage convention, same discipline as `presets.rs`'s
//! `radio-presets.json`).
//!
//! **Recon finding (plan 2 Task 3):** the plan's Task 3 recon step asked
//! whether a named "station-set"/"group" concept already exists anywhere in
//! the codebase for `@station-set:<name>` to resolve against. It does not.
//! `RelayFavorite`/`network_po_favorites` (`config.rs`) is the closest
//! superficial match by name, but it models a single Network Post Office
//! relay endpoint (one address + metadata), not a named collection of
//! ordinary station callsigns — bolting a routines dependency onto it would
//! be a category error, not reuse. The station-listing cache
//! (`catalog::stations`/`stations_cache`/`stations_disk`) and Find-a-Station
//! are a live poll + ranking result set, not an operator-curated named
//! group either. Per the plan's explicit fallback ("implement station-sets
//! as a NEW simple named-collection store... rather than bolting onto an
//! unrelated service"), this module is that new store: one flat
//! `station-sets.json` array of `{ name, callsigns }` objects, upsert-by-name,
//! atomic writes — the same shape and discipline as [`super::presets`].

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::atomic_write;

#[derive(Debug, thiserror::Error)]
pub enum StationSetError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("station-set not found: {0}")]
    NotFound(String),
}

/// A named, ordered collection of station callsigns (e.g. the OR-region
/// Winlink gateways an operator wants a routine to iterate). Order is
/// preserved on disk — routine actions that "try stations in order"
/// (`radio.connect`'s station×band iteration, plan 2 Task 4) depend on it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StationSet {
    pub name: String,
    pub callsigns: Vec<String>,
}

/// Stateless wrapper around `station-sets.json`: every method reads/writes
/// disk directly (no in-memory cache), matching `RadioPresetStore`'s
/// discipline so an `Arc<StationSetStore>` (the resolver's seam) never races
/// a cache gone stale relative to a concurrent `save`/`delete`.
pub struct StationSetStore {
    path: PathBuf,
}

impl StationSetStore {
    pub fn open(path: PathBuf) -> Self {
        Self { path }
    }

    fn read_all(&self) -> Vec<StationSet> {
        match std::fs::read_to_string(&self.path) {
            Ok(raw) => serde_json::from_str(&raw).unwrap_or_default(),
            Err(_) => Vec::new(),
        }
    }

    fn write_all(&self, sets: &[StationSet]) -> Result<(), StationSetError> {
        let json = serde_json::to_vec_pretty(sets)?;
        atomic_write(&self.path, &json)?;
        Ok(())
    }

    pub fn list(&self) -> Vec<StationSet> {
        self.read_all()
    }

    pub fn get(&self, name: &str) -> Option<StationSet> {
        self.read_all().into_iter().find(|s| s.name == name)
    }

    /// Create-or-update by `name` (upsert), matching `RadioPresetStore::save`.
    pub fn save(&self, set: &StationSet) -> Result<(), StationSetError> {
        let mut all = self.read_all();
        if let Some(existing) = all.iter_mut().find(|s| s.name == set.name) {
            *existing = set.clone();
        } else {
            all.push(set.clone());
        }
        self.write_all(&all)
    }

    pub fn delete(&self, name: &str) -> Result<(), StationSetError> {
        let mut all = self.read_all();
        let before = all.len();
        all.retain(|s| s.name != name);
        if all.len() == before {
            return Err(StationSetError::NotFound(name.to_string()));
        }
        self.write_all(&all)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(name: &str, callsigns: &[&str]) -> StationSet {
        StationSet {
            name: name.to_string(),
            callsigns: callsigns.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn save_then_get_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let store = StationSetStore::open(dir.path().join("station-sets.json"));
        let s = set("or-gateways", &["W7DEF-10", "K7ABC-10"]);
        store.save(&s).unwrap();
        assert_eq!(store.get("or-gateways"), Some(s));
    }

    #[test]
    fn save_upserts_existing_by_name() {
        let dir = tempfile::tempdir().unwrap();
        let store = StationSetStore::open(dir.path().join("station-sets.json"));
        store.save(&set("or-gateways", &["W7DEF-10"])).unwrap();
        store
            .save(&set("or-gateways", &["W7DEF-10", "K7ABC-10"]))
            .unwrap();

        let all = store.list();
        assert_eq!(all.len(), 1, "upsert must not duplicate by name");
        assert_eq!(all[0].callsigns, vec!["W7DEF-10", "K7ABC-10"]);
    }

    #[test]
    fn delete_removes_station_set() {
        let dir = tempfile::tempdir().unwrap();
        let store = StationSetStore::open(dir.path().join("station-sets.json"));
        store.save(&set("wa-gateways", &["W7ABC-1"])).unwrap();
        store.delete("wa-gateways").unwrap();
        assert!(store.get("wa-gateways").is_none());
        assert!(store.list().is_empty());
    }

    #[test]
    fn delete_missing_station_set_errors_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let store = StationSetStore::open(dir.path().join("station-sets.json"));
        let err = store.delete("ghost").unwrap_err();
        assert!(matches!(err, StationSetError::NotFound(name) if name == "ghost"));
    }

    #[test]
    fn list_and_reopen_round_trip_multiple_sets_preserving_order() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("station-sets.json");
        let store = StationSetStore::open(path.clone());
        store
            .save(&set("or-gateways", &["W7DEF-10", "K7ABC-10", "N7XYZ-1"]))
            .unwrap();
        store.save(&set("wa-gateways", &["W7ABC-1"])).unwrap();

        let reopened = StationSetStore::open(path);
        let or_set = reopened.get("or-gateways").expect("must round-trip");
        assert_eq!(or_set.callsigns, vec!["W7DEF-10", "K7ABC-10", "N7XYZ-1"]);
        let mut names: Vec<String> = reopened.list().into_iter().map(|s| s.name).collect();
        names.sort();
        assert_eq!(
            names,
            vec!["or-gateways".to_string(), "wa-gateways".to_string()]
        );
    }
}
