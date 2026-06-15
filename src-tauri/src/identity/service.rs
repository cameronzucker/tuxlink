//! `IdentityService` — mints `IdentityHandle`s after keyring validation.
//!
//! `authenticate(full, credential)` fetches the stored activation secret from
//! the OS keyring (account `tuxlink-identity-activation:<CALLSIGN-UPPER>`) and
//! constant-time-compares it (length-oracle-free, via SHA-256 digest + `subtle`)
//! to the entered `credential`. On match, it mints an `IdentityHandle` — the ONLY
//! path to a handle. `set_activation_secret`/`clear_activation_secret` manage the
//! keyring entry (add-time secret provisioning per resolved design decision #2).
//!
//! Keyring access goes through the `EntryLike` factory seam (reused from
//! `winlink::credentials`) so tests inject an in-memory mock with no OS keyring.

use subtle::ConstantTimeEq;

use super::address::Callsign;
use super::handle::IdentityHandle;
use super::keyring_keys::{activation_account, SERVICE};
use super::IdentityError;
use crate::winlink::credentials::EntryLike;

/// Factory closure type — matches `credentials.rs` / `station_password.rs` so the
/// production `real_factory` helper is reused.
pub type EntryFactory = Box<dyn Fn(&str, &str) -> Box<dyn EntryLike> + Send + Sync>;

/// Mints authentication handles after keyring validation.
pub struct IdentityService {
    factory: EntryFactory,
}

impl IdentityService {
    /// Production service backed by the real OS keyring.
    pub fn new() -> Self {
        let real_factory: EntryFactory = Box::new(|service: &str, account: &str| {
            let entry = keyring::Entry::new(service, account)
                .expect("keyring::Entry::new should not fail for valid service/account strings");
            Box::new(RealEntry(entry)) as Box<dyn EntryLike>
        });
        Self { factory: real_factory }
    }

    /// Test/injection constructor.
    pub fn with_factory(factory: EntryFactory) -> Self {
        Self { factory }
    }

    /// Validate `credential` against the stored activation secret for `full`, and
    /// on success mint an `IdentityHandle`.
    ///
    /// Errors: `NoSecretSet` (no keyring entry), `CredentialMismatch` (wrong
    /// secret), `Keyring` (backend error). The compare is constant-time +
    /// length-oracle-free (SHA-256 digest + `subtle::ConstantTimeEq`), matching
    /// `station_password::ct_eq_strings`.
    pub fn authenticate(
        &self,
        full: &Callsign,
        credential: &str,
    ) -> Result<IdentityHandle, IdentityError> {
        let account = activation_account(full.as_str());
        let entry = (self.factory)(SERVICE, &account);
        let stored = match entry.get_password() {
            Ok(s) => s,
            Err(keyring::Error::NoEntry) => return Err(IdentityError::NoSecretSet),
            Err(other) => return Err(IdentityError::Keyring(format!("{other}"))),
        };
        if ct_eq_strings(&stored, credential) {
            Ok(IdentityHandle::new(full.clone()))
        } else {
            Err(IdentityError::CredentialMismatch)
        }
    }

    /// Store (overwrite) the activation secret for `full` in the keyring.
    pub fn set_activation_secret(
        &self,
        full: &Callsign,
        secret: &str,
    ) -> Result<(), IdentityError> {
        let account = activation_account(full.as_str());
        let entry = (self.factory)(SERVICE, &account);
        entry
            .set_password(secret)
            .map_err(|e| IdentityError::Keyring(format!("{e}")))
    }

    /// Remove the activation secret for `full`. Idempotent: a missing entry is `Ok`.
    pub fn clear_activation_secret(&self, full: &Callsign) -> Result<(), IdentityError> {
        let account = activation_account(full.as_str());
        let entry = (self.factory)(SERVICE, &account);
        match entry.delete_password() {
            Ok(()) => Ok(()),
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(other) => Err(IdentityError::Keyring(format!("{other}"))),
        }
    }

    /// True iff an activation secret is stored in the keyring for `full`.
    pub fn has_activation_secret(&self, full: &Callsign) -> bool {
        let account = activation_account(full.as_str());
        let entry = (self.factory)(SERVICE, &account);
        entry.get_password().is_ok()
    }

    /// Fail-CLOSED activation-secret existence check (tuxlink-nx3g). Unlike
    /// [`has_activation_secret`] (which collapses backend errors to `false`),
    /// this distinguishes the three states the self-heal must not conflate:
    /// `Ok(true)` = present, `Ok(false)` = confirmed `NoEntry` (the orphan case),
    /// `Err(Keyring)` = backend read error (locked / unavailable). The heal MUST
    /// NOT treat a backend error as "absent" and then provision a secret.
    pub fn activation_secret_status(&self, full: &Callsign) -> Result<bool, IdentityError> {
        let account = activation_account(full.as_str());
        let entry = (self.factory)(SERVICE, &account);
        match entry.get_password() {
            Ok(_) => Ok(true),
            Err(keyring::Error::NoEntry) => Ok(false),
            Err(other) => Err(IdentityError::Keyring(format!("{other}"))),
        }
    }

    /// Self-heal an orphan-v2 FULL whose activation secret was never written
    /// (tuxlink-nx3g), by provisioning it FROM the trusted CMS password — restoring
    /// the `write_password` lockstep invariant. Returns `Ok(true)` if it healed,
    /// `Ok(false)` if no heal was needed/allowed (empty `cms_pw`, or a secret is
    /// already present), `Err` (fail-closed, no write) on a keyring backend error.
    ///
    /// SECURITY: the caller MUST have established trust in `cms_pw` — bootstrap reads
    /// it from the keyring (the trust root, no user input); the manual command proves
    /// it equals the stored CMS password (proof-of-knowledge). This never overwrites an
    /// EXISTING secret, so it structurally cannot touch a `CredentialMismatch`, and it
    /// never writes on a backend read error. Keeping this OUT of [`authenticate`] is
    /// what prevents "any typed string becomes the secret" (Codex R1 P0).
    pub fn heal_activation_secret(
        &self,
        full: &Callsign,
        cms_pw: &str,
    ) -> Result<bool, IdentityError> {
        if cms_pw.is_empty() {
            return Ok(false); // nothing trusted to copy from
        }
        if self.activation_secret_status(full)? {
            return Ok(false); // already present (or fail-closed on backend error) — never overwrite
        }
        self.set_activation_secret(full, cms_pw)?;
        Ok(true)
    }
}

impl Default for IdentityService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
impl IdentityService {
    /// Test-only: an in-memory-keyring service (no OS keyring). Cross-module
    /// test helper for Phase-2 migration + command tests.
    pub fn with_memory_keyring() -> Self {
        use std::collections::HashMap;
        use std::sync::{Arc, Mutex};
        let store: Arc<Mutex<HashMap<(String, String), String>>> = Arc::new(Mutex::new(HashMap::new()));
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
    store: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<(String, String), String>>>,
    service: String,
    account: String,
}
#[cfg(test)]
impl EntryLike for MemEntry {
    fn get_password(&self) -> Result<String, keyring::Error> {
        self.store.lock().unwrap().get(&(self.service.clone(), self.account.clone()))
            .cloned().ok_or(keyring::Error::NoEntry)
    }
    fn set_password(&self, password: &str) -> Result<(), keyring::Error> {
        self.store.lock().unwrap().insert((self.service.clone(), self.account.clone()), password.to_string());
        Ok(())
    }
    fn delete_password(&self) -> Result<(), keyring::Error> {
        if self.store.lock().unwrap().remove(&(self.service.clone(), self.account.clone())).is_some() { Ok(()) }
        else { Err(keyring::Error::NoEntry) }
    }
}

/// Real keyring entry wrapper (mirrors `credentials::RealEntry`).
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

/// Constant-time, length-oracle-free string equality (SHA-256 digest + `subtle`),
/// identical in approach to `station_password::ct_eq_strings`. `pub(crate)` so the
/// manual self-heal proof-of-knowledge compare (tuxlink-nx3g, commands.rs) reuses the
/// same constant-time path the auth gate uses.
pub(crate) fn ct_eq_strings(a: &str, b: &str) -> bool {
    use sha2::{Digest, Sha256};
    let digest_a = Sha256::digest(a.as_bytes());
    let digest_b = Sha256::digest(b.as_bytes());
    digest_a.as_slice().ct_eq(digest_b.as_slice()).unwrap_u8() == 1
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    // Canonical shared-HashMap mock (same shape as station_password.rs tests).
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

    fn mock_service() -> (IdentityService, MockStore) {
        let store: MockStore = Arc::new(Mutex::new(HashMap::new()));
        let store_for_factory = Arc::clone(&store);
        let factory: EntryFactory = Box::new(move |service: &str, account: &str| {
            Box::new(MockEntry {
                store: Arc::clone(&store_for_factory),
                service: service.to_string(),
                account: account.to_string(),
            }) as Box<dyn EntryLike>
        });
        (IdentityService::with_factory(factory), store)
    }

    fn call(s: &str) -> Callsign {
        Callsign::parse(s).unwrap()
    }

    #[test]
    fn authenticate_without_a_stored_secret_errors_no_secret_set() {
        let (svc, _store) = mock_service();
        assert_eq!(
            svc.authenticate(&call("W1ABC"), "anything").err(),
            Some(IdentityError::NoSecretSet)
        );
    }

    #[test]
    fn authenticate_with_correct_secret_mints_a_handle() {
        let (svc, _store) = mock_service();
        svc.set_activation_secret(&call("W1ABC"), "hunter2").unwrap();
        let handle = svc.authenticate(&call("W1ABC"), "hunter2").expect("correct secret");
        assert_eq!(handle.full_callsign().as_str(), "W1ABC");
    }

    #[test]
    fn authenticate_with_wrong_secret_errors_credential_mismatch() {
        let (svc, _store) = mock_service();
        svc.set_activation_secret(&call("W1ABC"), "hunter2").unwrap();
        assert_eq!(
            svc.authenticate(&call("W1ABC"), "wrong").err(),
            Some(IdentityError::CredentialMismatch)
        );
    }

    #[test]
    fn set_activation_secret_uses_the_documented_keyring_account() {
        let (svc, store) = mock_service();
        svc.set_activation_secret(&call("w1abc"), "hunter2").unwrap();
        let guard = store.lock().unwrap();
        assert_eq!(
            guard
                .get(&(
                    SERVICE.to_string(),
                    "tuxlink-identity-activation:W1ABC".to_string()
                ))
                .map(String::as_str),
            Some("hunter2"),
            "secret must be stored under tuxlink-identity-activation:<CALLSIGN-UPPER>"
        );
    }

    #[test]
    fn clear_activation_secret_then_authenticate_errors_no_secret_set() {
        let (svc, _store) = mock_service();
        svc.set_activation_secret(&call("W1ABC"), "hunter2").unwrap();
        svc.clear_activation_secret(&call("W1ABC")).unwrap();
        assert_eq!(
            svc.authenticate(&call("W1ABC"), "hunter2").err(),
            Some(IdentityError::NoSecretSet)
        );
    }

    #[test]
    fn clear_activation_secret_is_idempotent() {
        let (svc, _store) = mock_service();
        svc.clear_activation_secret(&call("W1ABC")).expect("clear-on-empty");
        svc.clear_activation_secret(&call("W1ABC")).expect("clear-twice");
    }

    #[test]
    fn authenticate_is_case_insensitive_on_the_keyring_account() {
        // The secret set under "w1abc" must authenticate when the Callsign is
        // parsed from "W1ABC" — the account string uppercases both sides.
        let (svc, _store) = mock_service();
        svc.set_activation_secret(&call("w1abc"), "hunter2").unwrap();
        assert!(svc.authenticate(&call("W1ABC"), "hunter2").is_ok());
    }

    // -- self-heal (tuxlink-nx3g) -------------------------------------------

    #[test]
    fn activation_secret_status_distinguishes_present_from_absent() {
        let (svc, _store) = mock_service();
        assert_eq!(svc.activation_secret_status(&call("W1ABC")), Ok(false), "absent");
        svc.set_activation_secret(&call("W1ABC"), "pw").unwrap();
        assert_eq!(svc.activation_secret_status(&call("W1ABC")), Ok(true), "present");
    }

    #[test]
    fn heal_provisions_the_secret_from_cms_pw_when_absent_then_authenticates() {
        let (svc, _store) = mock_service();
        // Orphan-v2: no activation secret yet.
        assert_eq!(svc.authenticate(&call("W1ABC"), "cms-pw").err(), Some(IdentityError::NoSecretSet));
        assert_eq!(svc.heal_activation_secret(&call("W1ABC"), "cms-pw"), Ok(true), "should heal");
        // Now authenticatable with the (copied) CMS password.
        assert!(svc.authenticate(&call("W1ABC"), "cms-pw").is_ok(), "healed identity authenticates");
    }

    #[test]
    fn heal_never_overwrites_an_existing_secret() {
        let (svc, _store) = mock_service();
        svc.set_activation_secret(&call("W1ABC"), "real-secret").unwrap();
        // CMS pw differs (e.g. a CredentialMismatch state) — heal must NOT overwrite.
        assert_eq!(svc.heal_activation_secret(&call("W1ABC"), "different"), Ok(false), "no-op");
        assert!(svc.authenticate(&call("W1ABC"), "real-secret").is_ok(), "original secret intact");
        assert_eq!(svc.authenticate(&call("W1ABC"), "different").err(), Some(IdentityError::CredentialMismatch));
    }

    #[test]
    fn heal_rejects_an_empty_cms_pw() {
        let (svc, _store) = mock_service();
        assert_eq!(svc.heal_activation_secret(&call("W1ABC"), ""), Ok(false), "empty pw never heals");
        assert_eq!(svc.activation_secret_status(&call("W1ABC")), Ok(false), "still no secret written");
    }

    #[test]
    fn heal_fails_closed_on_a_keyring_backend_error_and_does_not_write() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        // A factory whose reads error with a NON-NoEntry backend failure and whose
        // writes are counted — the heal MUST NOT treat the error as "absent" and write.
        let writes = Arc::new(AtomicUsize::new(0));
        let writes_for_factory = Arc::clone(&writes);
        let factory: EntryFactory = Box::new(move |_svc: &str, _account: &str| {
            Box::new(FailingEntry { writes: Arc::clone(&writes_for_factory) }) as Box<dyn EntryLike>
        });
        let svc = IdentityService::with_factory(factory);

        assert!(svc.activation_secret_status(&call("W1ABC")).is_err(), "backend error surfaces");
        assert!(svc.heal_activation_secret(&call("W1ABC"), "cms-pw").is_err(), "heal fails closed");
        assert_eq!(writes.load(Ordering::SeqCst), 0, "no provisioning write on a backend read error");
    }

    /// Reads always fail with a backend (non-NoEntry) error; writes are counted.
    struct FailingEntry {
        writes: Arc<std::sync::atomic::AtomicUsize>,
    }
    impl EntryLike for FailingEntry {
        fn get_password(&self) -> Result<String, keyring::Error> {
            Err(keyring::Error::PlatformFailure(Box::new(std::io::Error::new(
                std::io::ErrorKind::Other,
                "backend unavailable",
            ))))
        }
        fn set_password(&self, _password: &str) -> Result<(), keyring::Error> {
            self.writes.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        }
        fn delete_password(&self) -> Result<(), keyring::Error> {
            Ok(())
        }
    }
}
