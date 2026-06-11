// winlink_credentials_test.rs — unit tests for winlink::credentials (Task 7.1 / tuxlink-9phd)
// Spec: docs/superpowers/specs/2026-05-30-strip-pat-add-native-attachments.md §7.1
// Plan: Phase 7 Task 7.1
//
//! NOTE: These tests use an in-process HashMap-backed mock factory for keyring entries.
//!
//! We do NOT use `keyring::set_default_credential_builder(mock::default_credential_builder())`
//! here because the keyring 3.6.3 mock has `CredentialPersistence::EntryOnly` — every
//! `Entry::new()` call returns a fresh credential with no shared backing store. That makes
//! it impossible to test cross-entry scenarios (e.g. the P2P peer write-then-read-back
//! roundtrip) where one call writes and a later call must read it back.
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
    normalize_service_codes, p2p_peer_password_delete_with_factory,
    p2p_peer_password_read_with_factory, p2p_peer_password_write_with_factory,
    read_password_with_factory, service_codes_read_with_factory, service_codes_write_with_factory,
    EntryLike, KeyringError, DEFAULT_SERVICE_CODES,
};

// ──────────────────────────────────────────────────────────────
// Test-only mock infrastructure
// ──────────────────────────────────────────────────────────────

/// A fake credential entry backed by a shared `HashMap<(service, account), password>`.
///
/// Multiple `MockEntry` instances that share the same `Arc<Mutex<HashMap>>` see
/// each other's writes — exactly the cross-entry consistency the P2P roundtrip tests need.
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
// Test 2: Only the legacy "tuxlink-pat" entry exists → NoEntry.
// The legacy service is no longer consulted (Pat fully stripped, tuxlink-kc3q):
// read_password reads ONLY "tuxlink", so a stale legacy entry is ignored and
// left untouched (no read-through, no migration, no delete).
// ──────────────────────────────────────────────────────────────

#[test]
fn legacy_only_entry_is_ignored_returns_no_entry() {
    let store: Arc<Mutex<HashMap<(String, String), String>>> = Arc::new(Mutex::new(HashMap::new()));
    // Pre-populate ONLY the legacy service name.
    store.lock().unwrap().insert(
        ("tuxlink-pat".to_string(), "W4PHS".to_string()),
        "legacy_pass".to_string(),
    );

    let factory = mock_factory(Arc::clone(&store));
    let result = read_password_with_factory("W4PHS", &factory);

    assert!(
        matches!(&result, Err(KeyringError::NoEntry { callsign }) if callsign == "W4PHS"),
        "legacy 'tuxlink-pat' entry must be ignored (no migration), got: {result:?}"
    );

    // The legacy entry is left untouched and no canonical entry is created.
    let store_guard = store.lock().unwrap();
    assert_eq!(
        store_guard
            .get(&("tuxlink-pat".to_string(), "W4PHS".to_string()))
            .map(|s| s.as_str()),
        Some("legacy_pass"),
        "legacy entry must be left untouched (not read-through, not deleted)"
    );
    assert!(
        store_guard
            .get(&("tuxlink".to_string(), "W4PHS".to_string()))
            .is_none(),
        "no canonical 'tuxlink' entry should be created"
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

// ──────────────────────────────────────────────────────────────
// P2P peer password helpers (tuxlink-0pnb)
// ──────────────────────────────────────────────────────────────

#[test]
fn p2p_peer_password_roundtrip_in_keyring() {
    let store: Arc<Mutex<HashMap<(String, String), String>>> = Arc::new(Mutex::new(HashMap::new()));
    let factory = mock_factory(Arc::clone(&store));
    p2p_peer_password_write_with_factory("N7CPZ", "secretphrase", &factory).unwrap();
    let got = p2p_peer_password_read_with_factory("N7CPZ", &factory).unwrap();
    assert_eq!(got, "secretphrase");
}

#[test]
fn p2p_peer_password_delete_removes_entry() {
    let store: Arc<Mutex<HashMap<(String, String), String>>> = Arc::new(Mutex::new(HashMap::new()));
    let factory = mock_factory(Arc::clone(&store));
    p2p_peer_password_write_with_factory("N7CPZ", "x", &factory).unwrap();
    p2p_peer_password_delete_with_factory("N7CPZ", &factory).unwrap();
    let result = p2p_peer_password_read_with_factory("N7CPZ", &factory);
    assert!(matches!(result, Err(KeyringError::NoEntry { .. })));
}

#[test]
fn p2p_peer_password_keyring_account_uses_p2p_peer_prefix() {
    // The keyring 'account' field must be "p2p-peer:<CALLSIGN-UPPER>" so it
    // does not collide with the CMS-secure-login key namespace (just the callsign).
    let store: Arc<Mutex<HashMap<(String, String), String>>> = Arc::new(Mutex::new(HashMap::new()));
    let factory = mock_factory(Arc::clone(&store));
    p2p_peer_password_write_with_factory("n7cpz", "x", &factory).unwrap();
    // Reading via the CMS-side helper (which uses just the callsign as account)
    // should return NoEntry — proves no namespace collision.
    let cms_side = read_password_with_factory("N7CPZ", &factory);
    assert!(matches!(cms_side, Err(KeyringError::NoEntry { .. })));
}

// ──────────────────────────────────────────────────────────────
// Service codes (tuxlink-6j14)
// ──────────────────────────────────────────────────────────────

#[test]
fn service_codes_default_when_unset() {
    // No keyring entry yet → the read degrades to PUBLIC so station fetch works.
    let store: Arc<Mutex<HashMap<(String, String), String>>> = Arc::new(Mutex::new(HashMap::new()));
    let factory = mock_factory(Arc::clone(&store));
    assert_eq!(service_codes_read_with_factory(&factory), "PUBLIC");
}

#[test]
fn service_codes_write_then_read_roundtrips() {
    let store: Arc<Mutex<HashMap<(String, String), String>>> = Arc::new(Mutex::new(HashMap::new()));
    let factory = mock_factory(Arc::clone(&store));
    // A FOUO-shaped code the operator pasted; preserved verbatim (case-sensitive).
    service_codes_write_with_factory("PUBLIC EMCOMM", &factory).unwrap();
    assert_eq!(service_codes_read_with_factory(&factory), "PUBLIC EMCOMM");
}

#[test]
fn service_codes_write_normalizes_whitespace() {
    let store: Arc<Mutex<HashMap<(String, String), String>>> = Arc::new(Mutex::new(HashMap::new()));
    let factory = mock_factory(Arc::clone(&store));
    service_codes_write_with_factory("  PUBLIC   EMCOMM \n", &factory).unwrap();
    assert_eq!(service_codes_read_with_factory(&factory), "PUBLIC EMCOMM");
}

#[test]
fn service_codes_write_empty_falls_back_to_default() {
    // Clearing the field (empty / whitespace) must not produce an empty query —
    // it resets to PUBLIC.
    let store: Arc<Mutex<HashMap<(String, String), String>>> = Arc::new(Mutex::new(HashMap::new()));
    let factory = mock_factory(Arc::clone(&store));
    service_codes_write_with_factory("   ", &factory).unwrap();
    assert_eq!(service_codes_read_with_factory(&factory), DEFAULT_SERVICE_CODES);
}

#[test]
fn normalize_service_codes_cases() {
    assert_eq!(normalize_service_codes(""), "PUBLIC");
    assert_eq!(normalize_service_codes("   "), "PUBLIC");
    assert_eq!(normalize_service_codes("  PUBLIC  "), "PUBLIC");
    assert_eq!(normalize_service_codes("PUBLIC\tEMCOMM"), "PUBLIC EMCOMM");
    // Case preserved (FOUO codes may be case-sensitive; do not upcase).
    assert_eq!(normalize_service_codes("MixedCase99"), "MixedCase99");
}
