//! The persisted, secret-free identity list.
//!
//! `IdentityStore` is a Vec of FULL identities + a Vec of tactical identities +
//! a "last selected" UI hint. It holds NO secrets — activation secrets live
//! only in the OS keyring (see `service.rs`). Persisted as JSON next to
//! `config.json` (the store path is supplied by the caller; Phase 2 wires it to
//! `config_path()`'s sibling `identities.json`).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::address::{Address, Callsign};
use super::IdentityError;

/// A licensed identity — the security principal. Owns a mailbox (Phase 4) and a
/// keyring activation secret (the secret itself is NOT stored here).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FullIdentity {
    pub callsign: Callsign,
    /// Operator-friendly name, e.g. "Club". Optional.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// true => the activation secret is the CMS password; false => a local passphrase.
    pub has_cms_account: bool,
    /// The callsign's own account is CMS-registered.
    pub cms_registered: bool,
}

/// CMS-registration state of a tactical address (resolved design decision #3:
/// 24h TTL cache; Phase 5 owns the verification + caching).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TacticalCmsState {
    Unknown,
    Registered { checked_unix: u64 },
    NotRegistered { checked_unix: u64 },
}

/// A tactical label operating UNDER a parent FULL identity. No own credential,
/// no own mailbox.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TacticalIdentity {
    pub label: String,
    pub parent: Callsign,
    pub cms: TacticalCmsState,
}

/// Persisted identity list. NO secrets. `path` is the on-disk JSON location
/// (skipped from serialization — it is runtime state, not file content).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IdentityStore {
    #[serde(default)]
    full: Vec<FullIdentity>,
    #[serde(default)]
    tactical: Vec<TacticalIdentity>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    last_selected: Option<Address>,
    #[serde(skip)]
    path: PathBuf,
}

impl IdentityStore {
    /// Load the store from `path`. A missing file yields an empty store bound to
    /// that path (first-run); a present file is parsed. The `path` is retained so
    /// [`save`](Self::save) writes back to the same location.
    pub fn load(path: &Path) -> Result<Self, IdentityError> {
        match std::fs::read(path) {
            Ok(bytes) => {
                let mut store: IdentityStore = serde_json::from_slice(&bytes)
                    .map_err(|e| IdentityError::Io(format!("parse {}: {e}", path.display())))?;
                store.path = path.to_path_buf();
                Ok(store)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(IdentityStore {
                path: path.to_path_buf(),
                ..Default::default()
            }),
            Err(e) => Err(IdentityError::Io(format!("read {}: {e}", path.display()))),
        }
    }

    /// Persist the store to its bound `path` (pretty JSON, parent dirs created).
    pub fn save(&self) -> Result<(), IdentityError> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| IdentityError::Io(format!("mkdir {}: {e}", parent.display())))?;
        }
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| IdentityError::Io(format!("serialize: {e}")))?;
        std::fs::write(&self.path, json)
            .map_err(|e| IdentityError::Io(format!("write {}: {e}", self.path.display())))
    }

    pub fn full(&self) -> &[FullIdentity] {
        &self.full
    }

    pub fn tactical(&self) -> &[TacticalIdentity] {
        &self.tactical
    }

    pub fn full_by_callsign(&self, c: &Callsign) -> Option<&FullIdentity> {
        self.full.iter().find(|f| f.callsign == *c)
    }

    /// Add a FULL identity. Errors if a FULL with the same callsign already exists.
    pub fn add_full(&mut self, id: FullIdentity) -> Result<(), IdentityError> {
        if self.full_by_callsign(&id.callsign).is_some() {
            return Err(IdentityError::InvalidCallsign("duplicate FULL callsign".into()));
        }
        self.full.push(id);
        Ok(())
    }

    /// Add a tactical identity. Errors with `ParentNotFound` if its parent
    /// callsign is not a known FULL identity (the tactical-parent invariant).
    pub fn add_tactical(&mut self, t: TacticalIdentity) -> Result<(), IdentityError> {
        if self.full_by_callsign(&t.parent).is_none() {
            return Err(IdentityError::ParentNotFound);
        }
        self.tactical.push(t);
        Ok(())
    }

    /// Remove a FULL or tactical identity by address. Removing a FULL that still
    /// has tactical children errors with `RemoveHasTacticals`. Removing something
    /// that does not exist errors with `UnknownIdentity`.
    pub fn remove(&mut self, addr: &Address) -> Result<(), IdentityError> {
        match addr {
            Address::Full(c) => {
                if self.full_by_callsign(c).is_none() {
                    return Err(IdentityError::UnknownIdentity);
                }
                if self.tactical.iter().any(|t| t.parent == *c) {
                    return Err(IdentityError::RemoveHasTacticals);
                }
                self.full.retain(|f| f.callsign != *c);
                Ok(())
            }
            Address::Tactical(label) => {
                let before = self.tactical.len();
                self.tactical.retain(|t| t.label != *label);
                if self.tactical.len() == before {
                    Err(IdentityError::UnknownIdentity)
                } else {
                    Ok(())
                }
            }
        }
    }

    pub fn last_selected(&self) -> Option<&Address> {
        self.last_selected.as_ref()
    }

    pub fn set_last_selected(&mut self, addr: Address) {
        self.last_selected = Some(addr);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(s: &str) -> Callsign {
        Callsign::parse(s).unwrap()
    }

    fn full(s: &str) -> FullIdentity {
        FullIdentity {
            callsign: call(s),
            label: None,
            has_cms_account: false,
            cms_registered: false,
        }
    }

    fn tac(label: &str, parent: &str) -> TacticalIdentity {
        TacticalIdentity {
            label: label.to_string(),
            parent: call(parent),
            cms: TacticalCmsState::Unknown,
        }
    }

    #[test]
    fn load_missing_file_yields_empty_store_bound_to_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("identities.json");
        let store = IdentityStore::load(&path).expect("missing file => empty store");
        assert!(store.full().is_empty());
        assert!(store.tactical().is_empty());
        assert!(store.last_selected().is_none());
    }

    #[test]
    fn add_full_then_lookup_by_callsign() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = IdentityStore::load(&dir.path().join("identities.json")).unwrap();
        store.add_full(full("W1ABC")).unwrap();
        assert!(store.full_by_callsign(&call("W1ABC")).is_some());
        assert!(store.full_by_callsign(&call("W2XYZ")).is_none());
    }

    #[test]
    fn add_duplicate_full_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = IdentityStore::load(&dir.path().join("identities.json")).unwrap();
        store.add_full(full("W1ABC")).unwrap();
        assert!(store.add_full(full("W1ABC")).is_err());
    }

    #[test]
    fn add_tactical_requires_known_parent() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = IdentityStore::load(&dir.path().join("identities.json")).unwrap();
        assert_eq!(
            store.add_tactical(tac("EOC-3", "W1ABC")),
            Err(IdentityError::ParentNotFound),
            "tactical with an unknown parent must be rejected"
        );
        store.add_full(full("W1ABC")).unwrap();
        store.add_tactical(tac("EOC-3", "W1ABC")).expect("now parent exists");
        assert_eq!(store.tactical().len(), 1);
    }

    #[test]
    fn remove_full_with_tacticals_is_blocked() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = IdentityStore::load(&dir.path().join("identities.json")).unwrap();
        store.add_full(full("W1ABC")).unwrap();
        store.add_tactical(tac("EOC-3", "W1ABC")).unwrap();
        assert_eq!(
            store.remove(&Address::Full(call("W1ABC"))),
            Err(IdentityError::RemoveHasTacticals)
        );
        // Removing the tactical first then the FULL succeeds.
        store.remove(&Address::Tactical("EOC-3".into())).unwrap();
        store.remove(&Address::Full(call("W1ABC"))).unwrap();
        assert!(store.full().is_empty());
    }

    #[test]
    fn remove_unknown_address_errors() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = IdentityStore::load(&dir.path().join("identities.json")).unwrap();
        assert_eq!(
            store.remove(&Address::Full(call("W9NONE"))),
            Err(IdentityError::UnknownIdentity)
        );
        assert_eq!(
            store.remove(&Address::Tactical("GHOST".into())),
            Err(IdentityError::UnknownIdentity)
        );
    }

    #[test]
    fn save_then_load_round_trips_without_secrets() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("identities.json");
        let mut store = IdentityStore::load(&path).unwrap();
        store
            .add_full(FullIdentity {
                callsign: call("W1ABC"),
                label: Some("Home".into()),
                has_cms_account: true,
                cms_registered: true,
            })
            .unwrap();
        store.add_tactical(tac("EOC-3", "W1ABC")).unwrap();
        store.set_last_selected(Address::Full(call("W1ABC")));
        store.save().unwrap();

        // The on-disk JSON must not contain any secret material — only the
        // identity list. (Sanity: no "password"/"secret" keys leak from here.)
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(!raw.contains("password"), "store must hold NO secrets: {raw}");
        assert!(!raw.contains("secret"), "store must hold NO secrets: {raw}");

        let reloaded = IdentityStore::load(&path).unwrap();
        assert_eq!(reloaded.full().len(), 1);
        assert_eq!(reloaded.full()[0].label.as_deref(), Some("Home"));
        assert_eq!(reloaded.tactical().len(), 1);
        assert_eq!(
            reloaded.last_selected(),
            Some(&Address::Full(call("W1ABC")))
        );
    }

    #[test]
    fn set_last_selected_overwrites() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = IdentityStore::load(&dir.path().join("identities.json")).unwrap();
        store.set_last_selected(Address::Tactical("EOC-3".into()));
        store.set_last_selected(Address::Full(call("W1ABC")));
        assert_eq!(store.last_selected(), Some(&Address::Full(call("W1ABC"))));
    }
}
