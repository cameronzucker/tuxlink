//! Saved searches + recent search history, backed by a JSON file on disk.
//!
//! The caller is responsible for determining the file path
//! (`$APPCONFIG/saved-searches.json`); `SavedStore::open` takes a `PathBuf`.

use crate::search::types::QuerySpec;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;
use uuid::Uuid;

const SCHEMA_VERSION: u32 = 1;
pub const RECENT_CAP: usize = 20;

#[derive(Error, Debug)]
pub enum SavedError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("not found: {0}")]
    NotFound(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SavedSearch {
    pub id: String,               // uuid v4
    pub name: String,
    pub spec: QuerySpec,
    pub created_at: i64,          // unix seconds
    pub last_used_at: Option<i64>,
    pub order: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecentSearch {
    pub spec: QuerySpec,
    pub ran_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SavedStoreFile {
    pub version: u32,
    pub saved: Vec<SavedSearch>,
    pub recent: Vec<RecentSearch>,
}

impl Default for SavedStoreFile {
    fn default() -> Self {
        Self {
            version: SCHEMA_VERSION,
            saved: vec![],
            recent: vec![],
        }
    }
}

pub struct SavedStore {
    path: PathBuf,
    file: SavedStoreFile,
}

impl SavedStore {
    pub fn open(path: PathBuf) -> Result<Self, SavedError> {
        let file = if path.exists() {
            let raw = std::fs::read_to_string(&path)?;
            serde_json::from_str(&raw)?
        } else {
            SavedStoreFile::default()
        };
        Ok(Self { path, file })
    }

    fn flush(&self) -> Result<(), SavedError> {
        let json = serde_json::to_string_pretty(&self.file)?;
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&self.path, json)?;
        Ok(())
    }

    pub fn save(&mut self, name: &str, spec: QuerySpec, now: i64) -> Result<SavedSearch, SavedError> {
        let order = self
            .file
            .saved
            .iter()
            .map(|s| s.order)
            .max()
            .map(|n| n + 1)
            .unwrap_or(0);
        let s = SavedSearch {
            id: Uuid::new_v4().to_string(),
            name: name.to_string(),
            spec,
            created_at: now,
            last_used_at: None,
            order,
        };
        self.file.saved.push(s.clone());
        self.flush()?;
        Ok(s)
    }

    pub fn unsave(&mut self, id: &str) -> Result<(), SavedError> {
        let before = self.file.saved.len();
        self.file.saved.retain(|s| s.id != id);
        if self.file.saved.len() == before {
            return Err(SavedError::NotFound(id.to_string()));
        }
        self.flush()
    }

    pub fn rename(&mut self, id: &str, name: &str) -> Result<(), SavedError> {
        let s = self
            .file
            .saved
            .iter_mut()
            .find(|s| s.id == id)
            .ok_or_else(|| SavedError::NotFound(id.to_string()))?;
        s.name = name.to_string();
        self.flush()
    }

    pub fn reorder(&mut self, ordered_ids: &[String]) -> Result<(), SavedError> {
        for (i, id) in ordered_ids.iter().enumerate() {
            let s = self
                .file
                .saved
                .iter_mut()
                .find(|s| &s.id == id)
                .ok_or_else(|| SavedError::NotFound(id.clone()))?;
            s.order = i as u32;
        }
        self.file.saved.sort_by_key(|s| s.order);
        self.flush()
    }

    pub fn record_recent(&mut self, spec: QuerySpec, now: i64) -> Result<(), SavedError> {
        self.file.recent.insert(0, RecentSearch { spec, ran_at: now });
        if self.file.recent.len() > RECENT_CAP {
            self.file.recent.truncate(RECENT_CAP);
        }
        self.flush()
    }

    pub fn promote_recent(
        &mut self,
        name: &str,
        spec: &QuerySpec,
        now: i64,
    ) -> Result<SavedSearch, SavedError> {
        self.file.recent.retain(|r| &r.spec != spec);
        let saved = self.save(name, spec.clone(), now)?;
        Ok(saved)
    }

    pub fn saved(&self) -> &[SavedSearch] {
        &self.file.saved
    }

    pub fn recent(&self) -> &[RecentSearch] {
        &self.file.recent
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn empty_spec() -> QuerySpec {
        QuerySpec::default()
    }

    #[test]
    fn open_returns_default_when_file_missing() {
        let dir = tempdir().unwrap();
        let store = SavedStore::open(dir.path().join("saved-searches.json")).unwrap();
        assert!(store.saved().is_empty());
        assert!(store.recent().is_empty());
    }

    #[test]
    fn save_then_unsave_roundtrip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("saved.json");
        let mut store = SavedStore::open(path.clone()).unwrap();
        let s = store.save("Storm Net", empty_spec(), 1_700_000_000).unwrap();
        assert_eq!(store.saved().len(), 1);
        assert_eq!(s.name, "Storm Net");
        // reload from disk
        let store2 = SavedStore::open(path.clone()).unwrap();
        assert_eq!(store2.saved().len(), 1);
        // unsave
        let mut store3 = store2;
        store3.unsave(&s.id).unwrap();
        assert_eq!(store3.saved().len(), 0);
    }

    #[allow(non_snake_case)]
    #[test]
    fn record_recent_caps_at_RECENT_CAP() {
        let dir = tempdir().unwrap();
        let mut store = SavedStore::open(dir.path().join("s.json")).unwrap();
        for i in 0..(RECENT_CAP as i64 + 5) {
            store
                .record_recent(empty_spec(), 1_700_000_000 + i)
                .unwrap();
        }
        assert_eq!(store.recent().len(), RECENT_CAP);
        // newest first
        assert!(
            store.recent().first().unwrap().ran_at > store.recent().last().unwrap().ran_at
        );
    }

    #[test]
    fn promote_recent_moves_into_saved() {
        let dir = tempdir().unwrap();
        let mut store = SavedStore::open(dir.path().join("s.json")).unwrap();
        store.record_recent(empty_spec(), 1_700_000_000).unwrap();
        let s = store
            .promote_recent("My pick", &empty_spec(), 1_700_000_100)
            .unwrap();
        assert_eq!(s.name, "My pick");
        assert_eq!(store.saved().len(), 1);
        // promoted entry is removed from recent
        assert_eq!(store.recent().len(), 0);
    }
}
