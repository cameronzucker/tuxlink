//! `RadioPreset` entity: CRUD over a single `radio-presets.json` file that
//! lives beside `config.json` (spec §14). A Radio Preset names a
//! frequency/mode/power/ATU combination so a routine step (`@preset:<name>`,
//! resolved by `resolver.rs` in a later plan-2 task) and the Radio Presets UI
//! reference the same on-disk shape.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::atomic_write;

#[derive(Debug, thiserror::Error)]
pub enum PresetError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("preset not found: {0}")]
    NotFound(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RadioPreset {
    pub name: String,
    pub frequency_hz: u64,
    pub mode: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub power_w: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub atu: Option<bool>,
}

/// Stateless wrapper around `radio-presets.json`: every method reads/writes
/// disk directly (no in-memory cache), matching `DefinitionStore`'s
/// discipline so an `Arc<RadioPresetStore>` (the resolver's Task-3 seam)
/// never races a cache gone stale relative to a concurrent `save`/`delete`.
pub struct RadioPresetStore {
    path: PathBuf,
}

impl RadioPresetStore {
    pub fn open(path: PathBuf) -> Self {
        Self { path }
    }

    fn read_all(&self) -> Vec<RadioPreset> {
        match std::fs::read_to_string(&self.path) {
            Ok(raw) => serde_json::from_str(&raw).unwrap_or_default(),
            Err(_) => Vec::new(),
        }
    }

    fn write_all(&self, presets: &[RadioPreset]) -> Result<(), PresetError> {
        let json = serde_json::to_vec_pretty(presets)?;
        atomic_write(&self.path, &json)?;
        Ok(())
    }

    pub fn list(&self) -> Vec<RadioPreset> {
        self.read_all()
    }

    pub fn get(&self, name: &str) -> Option<RadioPreset> {
        self.read_all().into_iter().find(|p| p.name == name)
    }

    /// Create-or-update by `name` (upsert) — the Radio Presets UI edits an
    /// existing preset and creates a new one through the same command.
    pub fn save(&self, preset: &RadioPreset) -> Result<(), PresetError> {
        let mut all = self.read_all();
        if let Some(existing) = all.iter_mut().find(|p| p.name == preset.name) {
            *existing = preset.clone();
        } else {
            all.push(preset.clone());
        }
        self.write_all(&all)
    }

    pub fn delete(&self, name: &str) -> Result<(), PresetError> {
        let mut all = self.read_all();
        let before = all.len();
        all.retain(|p| p.name != name);
        if all.len() == before {
            return Err(PresetError::NotFound(name.to_string()));
        }
        self.write_all(&all)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn preset(name: &str, hz: u64) -> RadioPreset {
        RadioPreset {
            name: name.to_string(),
            frequency_hz: hz,
            mode: "USB".to_string(),
            power_w: Some(20),
            atu: Some(true),
        }
    }

    #[test]
    fn save_then_get_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let store = RadioPresetStore::open(dir.path().join("radio-presets.json"));
        let p = preset("40m-ardop", 7_070_000);
        store.save(&p).unwrap();
        assert_eq!(store.get("40m-ardop"), Some(p));
    }

    #[test]
    fn save_upserts_existing_by_name() {
        let dir = tempfile::tempdir().unwrap();
        let store = RadioPresetStore::open(dir.path().join("radio-presets.json"));
        store.save(&preset("40m-ardop", 7_070_000)).unwrap();
        store.save(&preset("40m-ardop", 7_073_000)).unwrap();

        let all = store.list();
        assert_eq!(all.len(), 1, "upsert must not duplicate by name");
        assert_eq!(all[0].frequency_hz, 7_073_000);
    }

    #[test]
    fn delete_removes_preset() {
        let dir = tempfile::tempdir().unwrap();
        let store = RadioPresetStore::open(dir.path().join("radio-presets.json"));
        store.save(&preset("80m-listen", 3_585_000)).unwrap();
        store.delete("80m-listen").unwrap();
        assert!(store.get("80m-listen").is_none());
        assert!(store.list().is_empty());
    }

    #[test]
    fn delete_missing_preset_errors_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let store = RadioPresetStore::open(dir.path().join("radio-presets.json"));
        let err = store.delete("ghost").unwrap_err();
        assert!(matches!(err, PresetError::NotFound(name) if name == "ghost"));
    }

    #[test]
    fn list_and_reopen_round_trip_multiple_presets() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("radio-presets.json");
        let store = RadioPresetStore::open(path.clone());
        store.save(&preset("40m-ardop", 7_070_000)).unwrap();
        store.save(&preset("80m-listen", 3_585_000)).unwrap();

        let reopened = RadioPresetStore::open(path);
        let mut names: Vec<String> = reopened.list().into_iter().map(|p| p.name).collect();
        names.sort();
        assert_eq!(
            names,
            vec!["40m-ardop".to_string(), "80m-listen".to_string()]
        );
    }
}
