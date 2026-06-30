//! Origin-keyed OS-keyring helpers for the Elmer model API key.
//!
//! The API key for each model origin is stored in the OS keyring under the
//! canonical `"tuxlink"` service (matching `winlink::credentials` and
//! `identity::keyring_keys`), with a per-origin account string built by
//! [`elmer_key_account`].
//!
//! # Key namespace
//!
//! Account format: `elmer-agent-api-key::<origin>`, e.g.
//! `elmer-agent-api-key::https://api.openai.com`. Each origin gets its own
//! keyring entry — cross-origin reuse is structurally impossible.
//!
//! # Fail-closed status
//!
//! [`ElmerKeyring::status`] distinguishes three states:
//! - `Present` — the keyring returned a value.
//! - `Absent` — the keyring returned `NoEntry`.
//! - `Unreadable` — the keyring returned any other (backend / locked) error.
//!
//! `Unreadable` is NEVER collapsed to `Absent`. A false `Absent` would silently
//! drop a working key and later send an unauthenticated request to a cloud
//! provider. Mirror of `identity::service::activation_secret_status`.
//!
//! # Test isolation
//!
//! `EntryFactory` is the same type alias as `identity::service::EntryFactory`
//! (both alias `Box<dyn Fn(&str, &str) -> Box<dyn EntryLike> + Send + Sync>`);
//! they share the `EntryLike` trait from `winlink::credentials`. Tests inject an
//! in-memory factory via [`ElmerKeyring::with_memory_keyring`].

use serde::{Deserialize, Serialize};

use crate::winlink::credentials::EntryLike;
use tuxlink_agent_frontend::ApiKey;

// ---------------------------------------------------------------------------
// Service constant
// ---------------------------------------------------------------------------

/// Canonical keyring service name. MUST match `winlink::credentials::SERVICE`
/// (`"tuxlink"`) and `identity::keyring_keys::SERVICE`. Defined locally (rather
/// than imported as `pub(crate)` from another module) per the brief's constraint:
/// do not create cross-module `pub(crate)` coupling for a string constant.
const SERVICE: &str = "tuxlink";

// ---------------------------------------------------------------------------
// EntryFactory — reused seam from identity::service
// ---------------------------------------------------------------------------

/// Factory closure type — matches `identity::service::EntryFactory` and
/// `winlink::credentials` test seam. Production callers use `ElmerKeyring::new`;
/// tests inject via `with_factory` or `with_memory_keyring`.
pub type EntryFactory = Box<dyn Fn(&str, &str) -> Box<dyn EntryLike> + Send + Sync>;

// ---------------------------------------------------------------------------
// KeyStatus — 3-state fail-closed result
// ---------------------------------------------------------------------------

/// Three-state presence indicator for the stored API key.
///
/// The distinction between `Absent` and `Unreadable` is load-bearing:
/// - `Absent` means `NoEntry` — the key has never been set for this origin.
/// - `Unreadable` means a backend error (locked keyring, unavailable daemon,
///   platform failure) — the key MAY exist but cannot be read right now.
///
/// A caller MUST NOT treat `Unreadable` as permission to overwrite or discard
/// the stored key. See [`ElmerKeyring::status`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum KeyStatus {
    /// A key is present and readable for this origin.
    Present,
    /// No key has been stored for this origin (`keyring::Error::NoEntry`).
    Absent,
    /// The keyring backend returned an error other than `NoEntry` (e.g. locked
    /// or unavailable). The key may exist but cannot be read right now.
    Unreadable,
}

// ---------------------------------------------------------------------------
// ElmerKeyring
// ---------------------------------------------------------------------------

/// Keyring helper for Elmer model API keys, keyed by model endpoint origin.
///
/// One `ElmerKeyring` instance covers all origins; the origin string is passed
/// to each method at call time and maps to a distinct keyring account string.
pub struct ElmerKeyring {
    factory: EntryFactory,
}

impl ElmerKeyring {
    /// Production instance backed by the real OS keyring.
    pub fn new() -> Self {
        let real_factory: EntryFactory = Box::new(|service: &str, account: &str| {
            let entry = keyring::Entry::new(service, account)
                .expect(
                    "keyring::Entry::new should not fail for valid service/account strings",
                );
            Box::new(RealEntry(entry)) as Box<dyn EntryLike>
        });
        Self { factory: real_factory }
    }

    /// Injection constructor — accepts any factory (used by tests).
    pub fn with_factory(factory: EntryFactory) -> Self {
        Self { factory }
    }

    // -----------------------------------------------------------------------
    // Core operations
    // -----------------------------------------------------------------------

    /// Read the API key for `origin`.
    ///
    /// Returns:
    /// - `Ok(Some(key))` — key found.
    /// - `Ok(None)` — no key stored for this origin (`NoEntry`).
    /// - `Err(KeyringError::Backend(_))` — backend error (locked / unavailable).
    pub fn read(
        &self,
        origin: &str,
    ) -> Result<Option<ApiKey>, crate::winlink::credentials::KeyringError> {
        let account = elmer_key_account(origin);
        let entry = (self.factory)(SERVICE, &account);
        match entry.get_password() {
            Ok(s) => Ok(Some(ApiKey::new(s))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(other) => {
                Err(crate::winlink::credentials::KeyringError::Backend(format!("{other}")))
            }
        }
    }

    /// Store (overwrite) the API key for `origin`.
    ///
    /// # Errors
    ///
    /// `KeyringError::Backend` on any backend failure.
    pub fn set(
        &self,
        origin: &str,
        key: &ApiKey,
    ) -> Result<(), crate::winlink::credentials::KeyringError> {
        let account = elmer_key_account(origin);
        let entry = (self.factory)(SERVICE, &account);
        entry
            .set_password(key.expose())
            .map_err(|e| crate::winlink::credentials::KeyringError::Backend(format!("{e}")))
    }

    /// Remove the API key for `origin`. Idempotent: a missing entry returns `Ok`.
    ///
    /// # Errors
    ///
    /// `KeyringError::Backend` on any backend failure other than `NoEntry`.
    pub fn clear(
        &self,
        origin: &str,
    ) -> Result<(), crate::winlink::credentials::KeyringError> {
        let account = elmer_key_account(origin);
        let entry = (self.factory)(SERVICE, &account);
        match entry.delete_password() {
            Ok(()) => Ok(()),
            Err(keyring::Error::NoEntry) => Ok(()), // idempotent
            Err(other) => {
                Err(crate::winlink::credentials::KeyringError::Backend(format!("{other}")))
            }
        }
    }

    /// Fail-closed presence check for the API key for `origin`.
    ///
    /// Maps keyring results to the three-state [`KeyStatus`]:
    /// - `Ok(_)` → `Present`
    /// - `Err(NoEntry)` → `Absent`
    /// - Any other error → `Unreadable`
    ///
    /// A backend error (locked keyring, unavailable daemon) is NEVER collapsed
    /// to `Absent` — that would silently discard a stored key and later send
    /// keyless requests to a cloud provider.
    pub fn status(&self, origin: &str) -> KeyStatus {
        let account = elmer_key_account(origin);
        let entry = (self.factory)(SERVICE, &account);
        match entry.get_password() {
            Ok(_) => KeyStatus::Present,
            Err(keyring::Error::NoEntry) => KeyStatus::Absent,
            Err(_) => KeyStatus::Unreadable,
        }
    }
}

impl Default for ElmerKeyring {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Account string builder
// ---------------------------------------------------------------------------

/// Build the keyring account string for an Elmer API key.
///
/// Format: `elmer-agent-api-key::<origin>`. The double-colon separator ensures
/// the origin (an arbitrary URL string) cannot collide with the fixed prefix
/// even when the origin itself contains colons (e.g. `https://api.openai.com`).
fn elmer_key_account(origin: &str) -> String {
    format!("elmer-agent-api-key::{origin}")
}

// ---------------------------------------------------------------------------
// Production Entry wrapper
// ---------------------------------------------------------------------------

struct RealEntry(keyring::Entry);

impl EntryLike for RealEntry {
    fn get_password(&self) -> Result<String, keyring::Error> {
        self.0.get_password()
    }
    fn set_password(&self, password: &str) -> Result<(), keyring::Error> {
        self.0.set_password(password)
    }
    fn delete_password(&self) -> Result<(), keyring::Error> {
        self.0.delete_credential()
    }
}

// ---------------------------------------------------------------------------
// Test helpers (in-memory keyring factory)
// ---------------------------------------------------------------------------

#[cfg(test)]
impl ElmerKeyring {
    /// Test-only constructor: in-memory HashMap keyring, no OS keyring.
    ///
    /// All entries are scoped by `(service, account)` key. Mirrors
    /// `identity::service::IdentityService::with_memory_keyring`.
    pub fn with_memory_keyring() -> Self {
        use std::collections::HashMap;
        use std::sync::{Arc, Mutex};
        let store: Arc<Mutex<HashMap<(String, String), String>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let factory: EntryFactory = Box::new(move |service: &str, account: &str| {
            Box::new(MemEntry {
                store: Arc::clone(&store),
                service: service.to_string(),
                account: account.to_string(),
            }) as Box<dyn EntryLike>
        });
        Self::with_factory(factory)
    }
}

#[cfg(test)]
struct MemEntry {
    store: std::sync::Arc<
        std::sync::Mutex<std::collections::HashMap<(String, String), String>>,
    >,
    service: String,
    account: String,
}

#[cfg(test)]
impl EntryLike for MemEntry {
    fn get_password(&self) -> Result<String, keyring::Error> {
        self.store
            .lock()
            .unwrap()
            .get(&(self.service.clone(), self.account.clone()))
            .cloned()
            .ok_or(keyring::Error::NoEntry)
    }
    fn set_password(&self, password: &str) -> Result<(), keyring::Error> {
        self.store.lock().unwrap().insert(
            (self.service.clone(), self.account.clone()),
            password.to_string(),
        );
        Ok(())
    }
    fn delete_password(&self) -> Result<(), keyring::Error> {
        let key = (self.service.clone(), self.account.clone());
        if self.store.lock().unwrap().remove(&key).is_some() {
            Ok(())
        } else {
            Err(keyring::Error::NoEntry)
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    // -----------------------------------------------------------------------
    // FailingEntry — reads always return a non-NoEntry backend error.
    // Used to verify the fail-closed Unreadable path.
    // -----------------------------------------------------------------------

    struct FailingEntry {
        writes: Arc<std::sync::atomic::AtomicUsize>,
    }

    impl EntryLike for FailingEntry {
        fn get_password(&self) -> Result<String, keyring::Error> {
            Err(keyring::Error::PlatformFailure(Box::new(
                std::io::Error::other("backend unavailable"),
            )))
        }
        fn set_password(&self, _password: &str) -> Result<(), keyring::Error> {
            self.writes
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        }
        fn delete_password(&self) -> Result<(), keyring::Error> {
            Ok(())
        }
    }

    fn failing_keyring() -> (ElmerKeyring, Arc<std::sync::atomic::AtomicUsize>) {
        let writes = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let writes_for_factory = Arc::clone(&writes);
        let factory: EntryFactory = Box::new(move |_svc: &str, _account: &str| {
            Box::new(FailingEntry {
                writes: Arc::clone(&writes_for_factory),
            }) as Box<dyn EntryLike>
        });
        (ElmerKeyring::with_factory(factory), writes)
    }

    // -----------------------------------------------------------------------
    // MockEntry with shared HashMap — for account_is_origin_scoped assertion
    // -----------------------------------------------------------------------

    struct MockEntry {
        store: Arc<Mutex<HashMap<(String, String), String>>>,
        service: String,
        account: String,
    }

    impl EntryLike for MockEntry {
        fn get_password(&self) -> Result<String, keyring::Error> {
            self.store
                .lock()
                .unwrap()
                .get(&(self.service.clone(), self.account.clone()))
                .cloned()
                .ok_or(keyring::Error::NoEntry)
        }
        fn set_password(&self, password: &str) -> Result<(), keyring::Error> {
            self.store.lock().unwrap().insert(
                (self.service.clone(), self.account.clone()),
                password.to_string(),
            );
            Ok(())
        }
        fn delete_password(&self) -> Result<(), keyring::Error> {
            let key = (self.service.clone(), self.account.clone());
            if self.store.lock().unwrap().remove(&key).is_some() {
                Ok(())
            } else {
                Err(keyring::Error::NoEntry)
            }
        }
    }

    type MockStore = Arc<Mutex<HashMap<(String, String), String>>>;

    fn mock_keyring() -> (ElmerKeyring, MockStore) {
        let store: MockStore = Arc::new(Mutex::new(HashMap::new()));
        let store_for_factory = Arc::clone(&store);
        let factory: EntryFactory = Box::new(move |service: &str, account: &str| {
            Box::new(MockEntry {
                store: Arc::clone(&store_for_factory),
                service: service.to_string(),
                account: account.to_string(),
            }) as Box<dyn EntryLike>
        });
        (ElmerKeyring::with_factory(factory), store)
    }

    // -----------------------------------------------------------------------
    // Tests
    // -----------------------------------------------------------------------

    /// `set` then `read` round-trips the key value exactly.
    #[test]
    fn set_then_read_roundtrips() {
        let kr = ElmerKeyring::with_memory_keyring();
        let origin = "https://api.openai.com";
        let key = ApiKey::new("sk-x");
        kr.set(origin, &key).expect("set should succeed");
        let read_back = kr.read(origin).expect("read should succeed");
        let k = read_back.expect("key should be present after set");
        assert_eq!(k.expose(), "sk-x", "read-back value must match the stored key");
    }

    /// `read` on an empty store returns `Ok(None)` — not an error, not a value.
    #[test]
    fn read_absent_is_none() {
        let kr = ElmerKeyring::with_memory_keyring();
        let result = kr.read("https://x").expect("read on empty store must not error");
        assert!(result.is_none(), "absent key must be Ok(None), not Some(_)");
    }

    /// `status` is `Absent` before any `set`, `Present` after `set`.
    #[test]
    fn status_present_absent() {
        let kr = ElmerKeyring::with_memory_keyring();
        let origin = "https://api.openai.com";

        assert_eq!(
            kr.status(origin),
            KeyStatus::Absent,
            "status must be Absent before any set"
        );

        kr.set(origin, &ApiKey::new("sk-test")).expect("set");
        assert_eq!(
            kr.status(origin),
            KeyStatus::Present,
            "status must be Present after set"
        );
    }

    /// A backend error (non-NoEntry) MUST produce `Unreadable`, NEVER `Absent`.
    ///
    /// This is the critical fail-closed property: collapsing a locked / unavailable
    /// keyring to `Absent` would let callers silently discard a stored key and
    /// send unauthenticated requests to a cloud provider.
    #[test]
    fn status_unreadable_on_backend_error() {
        let (kr, _writes) = failing_keyring();
        let status = kr.status("https://api.openai.com");
        assert_eq!(
            status,
            KeyStatus::Unreadable,
            "a backend error MUST be Unreadable, never Absent — a false Absent drops a working key"
        );
    }

    /// `clear` on an empty store is `Ok` (idempotent). `clear` called twice is
    /// also `Ok`.
    #[test]
    fn clear_is_idempotent() {
        let kr = ElmerKeyring::with_memory_keyring();
        let origin = "https://api.openai.com";

        // Clear on empty — must not error.
        kr.clear(origin).expect("clear on empty must be Ok");

        // Set then clear twice.
        kr.set(origin, &ApiKey::new("sk-tmp")).expect("set");
        kr.clear(origin).expect("first clear");
        kr.clear(origin).expect("second clear — must be idempotent");

        // Confirm entry is gone.
        assert!(
            kr.read(origin).expect("read after clear").is_none(),
            "key must be absent after clear"
        );
    }

    /// `set` for one origin does NOT make the key readable for a different origin.
    /// Also asserts the exact stored account string.
    #[test]
    fn account_is_origin_scoped() {
        let openai = "https://api.openai.com";
        let openrouter = "https://openrouter.ai";

        let (kr, store) = mock_keyring();
        let key = ApiKey::new("sk-openai");
        kr.set(openai, &key).expect("set");

        // Cross-origin read must return None.
        let cross = kr.read(openrouter).expect("cross-origin read must not error");
        assert!(
            cross.is_none(),
            "a key set for {openai:?} must not be readable under {openrouter:?}"
        );

        // Assert the exact stored account string.
        let guard = store.lock().unwrap();
        let expected_account = "elmer-agent-api-key::https://api.openai.com".to_string();
        assert_eq!(
            guard
                .get(&(SERVICE.to_string(), expected_account.clone()))
                .map(String::as_str),
            Some("sk-openai"),
            "key must be stored under account {expected_account:?} in service {SERVICE:?}"
        );
    }

    // -----------------------------------------------------------------------
    // elmer_key_account unit check
    // -----------------------------------------------------------------------

    #[test]
    fn elmer_key_account_format() {
        assert_eq!(
            elmer_key_account("https://api.openai.com"),
            "elmer-agent-api-key::https://api.openai.com"
        );
        assert_eq!(
            elmer_key_account("https://openrouter.ai"),
            "elmer-agent-api-key::https://openrouter.ai"
        );
    }
}
