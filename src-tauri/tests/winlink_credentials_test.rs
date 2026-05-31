// winlink_credentials_test.rs — unit tests for winlink::credentials (Task 7.1 / tuxlink-9phd)
// Spec: docs/superpowers/specs/2026-05-30-strip-pat-add-native-attachments.md §7.1
// Plan: Phase 7 Task 7.1
//
//! NOTE: These tests use an in-process HashMap-backed mock factory for keyring entries.
//!
//! We do NOT use `keyring::set_default_credential_builder(mock::default_credential_builder())`
//! here because the keyring 3.6.3 mock has `CredentialPersistence::EntryOnly` — every
//! `Entry::new()` call returns a fresh credential with no shared backing store. That makes
//! it impossible to test the migration scenario, where `read_password` writes to a new
//! `Entry::new("tuxlink", call)` and then a caller should be able to read it back.
//!
//! The factory approach injects a `Fn(&str, &str) -> Box<dyn EntryLike>` that is backed by
//! an `Arc<Mutex<HashMap<(String, String), String>>>`. The same HashMap instance is shared
//! across all factory invocations within a test, providing the cross-entry state the mock
//! builder cannot. Production code uses the real `keyring::Entry` factory.
//!
//! ISOLATION GUARANTEE: No test in this file calls `keyring::Entry::new` (the real OS
//! credential backend). Every call to `read_password_with_factory` in this file uses
//! a `HashMap`-backed `MockEntry`. The real OS keyring is never contacted.
//!
//! Threading: tests use `#[serial]` only when they need the XDG guard; keyring state
//! is per-factory-instance (not process-global) so concurrency is not a concern here.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tuxlink_lib::winlink::credentials::{
    read_password_with_factory, EntryLike, KeyringError,
};

// ──────────────────────────────────────────────────────────────
// Test-only mock infrastructure
// ──────────────────────────────────────────────────────────────

/// A fake credential entry backed by a shared `HashMap<(service, account), password>`.
///
/// Multiple `MockEntry` instances that share the same `Arc<Mutex<HashMap>>` see
/// each other's writes — exactly the cross-entry consistency the migration test needs.
struct MockEntry {
    store: Arc<Mutex<HashMap<(String, String), String>>>,
    service: String,
    account: String,
}

impl EntryLike for MockEntry {
    fn get_password(&self) -> Result<String, keyring::Error> {
        let store = self.store.lock().unwrap();
        store
            .get(&(self.service.clone(), self.account.clone()))
            .cloned()
            .ok_or(keyring::Error::NoEntry)
    }

    fn set_password(&self, password: &str) -> Result<(), keyring::Error> {
        let mut store = self.store.lock().unwrap();
        store.insert(
            (self.service.clone(), self.account.clone()),
            password.to_string(),
        );
        Ok(())
    }

    fn delete_password(&self) -> Result<(), keyring::Error> {
        let mut store = self.store.lock().unwrap();
        let key = (self.service.clone(), self.account.clone());
        if store.remove(&key).is_some() {
            Ok(())
        } else {
            Err(keyring::Error::NoEntry)
        }
    }
}

/// Build a factory closure that always consults the given shared store.
fn mock_factory(
    store: Arc<Mutex<HashMap<(String, String), String>>>,
) -> impl Fn(&str, &str) -> Box<dyn EntryLike> {
    move |service: &str, account: &str| -> Box<dyn EntryLike> {
        Box::new(MockEntry {
            store: Arc::clone(&store),
            service: service.to_string(),
            account: account.to_string(),
        })
    }
}

// ──────────────────────────────────────────────────────────────
// Test 1: New entry ("tuxlink" service) exists → return its password
// ──────────────────────────────────────────────────────────────

#[test]
fn new_entry_exists_returns_password() {
    let store: Arc<Mutex<HashMap<(String, String), String>>> = Arc::new(Mutex::new(HashMap::new()));
    // Pre-populate the new service name
    store
        .lock()
        .unwrap()
        .insert(("tuxlink".to_string(), "W4PHS".to_string()), "hunter2".to_string());

    let factory = mock_factory(Arc::clone(&store));
    let result = read_password_with_factory("W4PHS", &factory);

    assert!(result.is_ok(), "expected Ok, got: {result:?}");
    assert_eq!(result.unwrap(), "hunter2");
}

// ──────────────────────────────────────────────────────────────
// Test 2: Only old entry ("tuxlink-pat") exists → migrates and returns password
// ──────────────────────────────────────────────────────────────

#[test]
fn old_entry_only_migrates_to_new_and_returns_password() {
    let store: Arc<Mutex<HashMap<(String, String), String>>> = Arc::new(Mutex::new(HashMap::new()));
    // Pre-populate only the legacy service name
    store.lock().unwrap().insert(
        ("tuxlink-pat".to_string(), "W4PHS".to_string()),
        "legacy_pass".to_string(),
    );

    let factory = mock_factory(Arc::clone(&store));
    let result = read_password_with_factory("W4PHS", &factory);

    assert!(result.is_ok(), "expected Ok after migration, got: {result:?}");
    assert_eq!(result.unwrap(), "legacy_pass");

    // Verify migration side-effects: new entry written, old entry deleted
    let store_guard = store.lock().unwrap();

    assert_eq!(
        store_guard.get(&("tuxlink".to_string(), "W4PHS".to_string())).map(|s| s.as_str()),
        Some("legacy_pass"),
        "new entry must be written during migration"
    );
    assert!(
        store_guard
            .get(&("tuxlink-pat".to_string(), "W4PHS".to_string()))
            .is_none(),
        "old entry must be deleted after migration"
    );
}

// ──────────────────────────────────────────────────────────────
// Test 3: Neither entry exists → KeyringError::NoEntry
// ──────────────────────────────────────────────────────────────

#[test]
fn neither_entry_exists_returns_no_entry_error() {
    let store: Arc<Mutex<HashMap<(String, String), String>>> = Arc::new(Mutex::new(HashMap::new()));

    let factory = mock_factory(Arc::clone(&store));
    let result = read_password_with_factory("W4PHS", &factory);

    assert!(result.is_err(), "expected Err, got Ok");
    match result.unwrap_err() {
        KeyringError::NoEntry { callsign } => {
            assert_eq!(callsign, "W4PHS", "callsign in error must match")
        }
        other => panic!("expected NoEntry, got: {other:?}"),
    }
}

// ──────────────────────────────────────────────────────────────
// Test 4: Backend error on primary read passes through as KeyringError::Backend
// ──────────────────────────────────────────────────────────────

#[test]
fn backend_error_on_primary_read_passes_through() {
    // Inject an entry that returns a platform failure on get_password.
    // We implement a special error-injecting MockEntry for this case.
    struct ErrorEntry;
    impl EntryLike for ErrorEntry {
        fn get_password(&self) -> Result<String, keyring::Error> {
            Err(keyring::Error::PlatformFailure("simulated backend failure".into()))
        }
        fn set_password(&self, _: &str) -> Result<(), keyring::Error> {
            Ok(())
        }
        fn delete_password(&self) -> Result<(), keyring::Error> {
            Ok(())
        }
    }

    let factory = |_service: &str, _account: &str| -> Box<dyn EntryLike> { Box::new(ErrorEntry) };

    let result = read_password_with_factory("W4PHS", &factory);

    assert!(result.is_err(), "expected Err, got Ok");
    match result.unwrap_err() {
        KeyringError::Backend(_) => {} // correct
        other => panic!("expected Backend error, got: {other:?}"),
    }
}
