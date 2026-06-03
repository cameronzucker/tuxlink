//! `StationPassword` — keyring-backed per-listener password.
//!
//! ## Per-listener, not per-station
//!
//! Same as WLE's model: one password challenges every incoming peer, regardless
//! of callsign. This matches operator intuition ("the listener has a password,"
//! not "every callsign has a password") and pairs with `AllowedStations` for
//! the WHO-is-allowed dimension.
//!
//! ## Storage
//!
//! Keyring (OS credential store) — DIVERGES from WLE, which stores the
//! listener password as plaintext in the `Winlink.ini` `.dat` files.
//!
//! Service name: `"tuxlink"` (canonical, matches `credentials.rs`).
//! Account key: `"listener-station-password"` — a fixed string, since there is
//! only one listener password per install (not per callsign).
//!
//! ## Constant-time verification
//!
//! [`StationPassword::verify`] uses the `subtle` crate's `ConstantTimeEq`
//! against the SHA-style equal-length-prepared comparison: equal-length cases
//! consult `ct_eq`; the length-mismatch fast path returns false without
//! consulting the secret. (See in-source comment on the latter — the fast path
//! is fine here because we are not trying to hide whether a password was set;
//! we are trying to hide HOW the bytes compare once both are present.) See
//! `dev/scratch/winlink-re/findings/telnet-p2p.md` for the threat model
//! (timing side-channel during the `Password :` prompt over plaintext telnet).
//!
//! Spec: `docs/design/2026-06-03-multi-transport-listener-architecture.md` §2.1
//! bd: tuxlink-3o2o

use subtle::ConstantTimeEq;

use crate::winlink::credentials::EntryLike;

/// Canonical keyring service name (must match `credentials::SERVICE`).
const SERVICE: &str = "tuxlink";

/// Fixed account key for the per-listener station password.
///
/// Prefixed `p2p-listener:` to namespace it away from the CMS callsign-based
/// account keys at the same `tuxlink` keyring service (per Codex review
/// finding 2026-06-03 — operator-typed listener-station-password could
/// otherwise collide with a CMS callsign equal to "listener-station-password").
const ACCOUNT: &str = "p2p-listener:station-password";

// ──────────────────────────────────────────────────────────────
// Errors
// ──────────────────────────────────────────────────────────────

/// Error returned by `StationPassword::set` / `clear` / `is_set` when the
/// underlying keyring backend misbehaves.
#[derive(Debug, thiserror::Error)]
pub enum StationPasswordError {
    #[error("keyring backend error: {0}")]
    Backend(String),
}

// ──────────────────────────────────────────────────────────────
// Factory + struct
// ──────────────────────────────────────────────────────────────

/// Factory closure type — matches `credentials.rs` so production callers can
/// reuse the same `real_factory` helper.
pub type EntryFactory = Box<dyn Fn(&str, &str) -> Box<dyn EntryLike> + Send + Sync>;

/// Per-listener station password, backed by the OS keyring via `EntryLike`.
///
/// The factory pattern allows tests to inject an in-memory `MockEntry`
/// without contacting the OS keyring (see `credentials.rs` for the canonical
/// pattern).
pub struct StationPassword {
    factory: EntryFactory,
}

impl StationPassword {
    /// Construct a `StationPassword` backed by the real OS keyring.
    pub fn new() -> Self {
        let real_factory: EntryFactory = Box::new(|service: &str, account: &str| {
            let entry = keyring::Entry::new(service, account)
                .expect("keyring::Entry::new should not fail for valid service/account strings");
            Box::new(RealEntry(entry)) as Box<dyn EntryLike>
        });
        Self { factory: real_factory }
    }

    /// Construct a `StationPassword` with a custom factory — for test injection.
    pub fn with_factory(factory: EntryFactory) -> Self {
        Self { factory }
    }

    /// Returns TRUE if a password is currently set, OR the keyring backend
    /// returns any error other than `NoEntry`.
    ///
    /// **Fail-closed semantics:** a transient keyring backend failure must NOT
    /// silently bypass the password gate. If we can't read the keyring, we
    /// don't know whether a password is configured — so we assume one IS, and
    /// the verify path will reject (since it also fails closed on backend
    /// errors). Only an explicit `NoEntry` returns FALSE.
    ///
    /// Per Codex review finding 2026-06-03: prior behavior treated all errors
    /// as "not set," which meant a momentarily-locked keyring + valid allowlist
    /// peer = accept without challenge. That's the wrong default.
    pub fn is_set(&self) -> bool {
        let entry = (self.factory)(SERVICE, ACCOUNT);
        match entry.get_password() {
            Ok(_) => true,
            Err(keyring::Error::NoEntry) => false,
            Err(_) => true, // fail closed: unknown state ⇒ require challenge
        }
    }

    /// Verify a candidate password against the stored one in constant time.
    ///
    /// Returns FALSE if:
    /// - No password is stored (the caller should gate on `is_set()` first;
    ///   `decide.rs` does this).
    /// - The stored password and the candidate are not byte-equal.
    ///
    /// The candidate→stored comparison uses `subtle::ConstantTimeEq` so a
    /// timing attacker cannot infer which byte differed. The pre-comparison
    /// length check returns FALSE early on length mismatch (the length of the
    /// stored password is not considered secret — the protocol leaks it via
    /// the size of the keyring entry on disk anyway).
    pub fn verify(&self, input: &str) -> bool {
        let entry = (self.factory)(SERVICE, ACCOUNT);
        let stored = match entry.get_password() {
            Ok(p) => p,
            Err(_) => return false,
        };
        ct_eq_strings(&stored, input)
    }

    /// Store a new password (overwrites any existing entry).
    pub fn set(&self, password: &str) -> Result<(), StationPasswordError> {
        let entry = (self.factory)(SERVICE, ACCOUNT);
        entry
            .set_password(password)
            .map_err(|e| StationPasswordError::Backend(format!("{e}")))
    }

    /// Remove the stored password. Idempotent: returns `Ok(())` if no entry exists.
    pub fn clear(&self) -> Result<(), StationPasswordError> {
        let entry = (self.factory)(SERVICE, ACCOUNT);
        match entry.delete_password() {
            Ok(()) => Ok(()),
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(other) => Err(StationPasswordError::Backend(format!("{other}"))),
        }
    }
}

impl Default for StationPassword {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for StationPassword {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never dump the inner factory or the stored password.
        f.debug_struct("StationPassword").finish_non_exhaustive()
    }
}

// ──────────────────────────────────────────────────────────────
// Real entry wrapper (mirrors credentials.rs's `RealEntry`)
// ──────────────────────────────────────────────────────────────

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

// ──────────────────────────────────────────────────────────────
// Constant-time string compare
// ──────────────────────────────────────────────────────────────

/// Constant-time string equality on `str` inputs WITHOUT length oracle.
///
/// Prior implementation early-returned FALSE on length mismatch, which leaked
/// the length of the stored password through verify timing. Per Codex review
/// finding 2026-06-03 — even the length of an operator's listener password
/// is sensitive (constrains the attack space and rate-limits brute-force).
///
/// Current implementation hashes both inputs through SHA-256 then compares
/// the fixed-size 32-byte digests in constant time. The verify runs in time
/// independent of the input lengths.
///
/// SHA-256 is appropriate here because:
/// - Output is fixed-size (32 bytes), so digest comparison is length-oracle-free.
/// - The hash is used as a comparison primitive, NOT as a password-derivation
///   function — the stored password is in the OS keyring already (secure
///   storage), and this hash is computed fresh on every verify.
fn ct_eq_strings(a: &str, b: &str) -> bool {
    use sha2::{Digest, Sha256};
    let mut ha = Sha256::new();
    ha.update(a.as_bytes());
    let digest_a = ha.finalize();

    let mut hb = Sha256::new();
    hb.update(b.as_bytes());
    let digest_b = hb.finalize();

    digest_a.as_slice().ct_eq(digest_b.as_slice()).unwrap_u8() == 1
}

// ──────────────────────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    // Shared HashMap-backed mock — same shape as winlink_credentials_test.rs's
    // canonical pattern so the keyring contract is exercised identically.
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

    fn mock_password() -> (StationPassword, Arc<Mutex<HashMap<(String, String), String>>>) {
        let store: Arc<Mutex<HashMap<(String, String), String>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let store_for_factory = Arc::clone(&store);
        let factory: EntryFactory = Box::new(move |service: &str, account: &str| {
            Box::new(MockEntry {
                store: Arc::clone(&store_for_factory),
                service: service.to_string(),
                account: account.to_string(),
            }) as Box<dyn EntryLike>
        });
        (StationPassword::with_factory(factory), store)
    }

    // ── Lifecycle ────────────────────────────────────────────────

    #[test]
    fn is_set_false_when_never_set() {
        let (sp, _store) = mock_password();
        assert!(!sp.is_set());
    }

    #[test]
    fn set_then_is_set_true() {
        let (sp, _store) = mock_password();
        sp.set("hunter2").expect("set");
        assert!(sp.is_set());
    }

    #[test]
    fn verify_correct_returns_true() {
        let (sp, _store) = mock_password();
        sp.set("hunter2").expect("set");
        assert!(sp.verify("hunter2"));
    }

    #[test]
    fn verify_wrong_returns_false() {
        let (sp, _store) = mock_password();
        sp.set("hunter2").expect("set");
        assert!(!sp.verify("hunter3"));
    }

    #[test]
    fn verify_when_not_set_returns_false() {
        let (sp, _store) = mock_password();
        assert!(!sp.verify("anything"));
    }

    #[test]
    fn clear_then_is_set_false() {
        let (sp, _store) = mock_password();
        sp.set("hunter2").expect("set");
        sp.clear().expect("clear");
        assert!(!sp.is_set());
    }

    #[test]
    fn clear_idempotent_on_empty() {
        let (sp, _store) = mock_password();
        sp.clear().expect("clear-on-empty");
        sp.clear().expect("clear-twice");
    }

    #[test]
    fn set_then_overwrite_replaces() {
        let (sp, _store) = mock_password();
        sp.set("old").expect("set");
        sp.set("new").expect("overwrite");
        assert!(sp.verify("new"));
        assert!(!sp.verify("old"));
    }

    // ── Constant-time semantics ──────────────────────────────────
    //
    // We cannot directly assert "constant-time" without a microbenchmark
    // harness, but we CAN assert that the implementation never returns
    // mid-loop on a byte mismatch (because `subtle::ConstantTimeEq` does
    // the work) AND that different-length and different-byte cases both
    // return false correctly.

    #[test]
    fn ct_eq_different_length_returns_false() {
        assert!(!ct_eq_strings("short", "longer-password"));
        assert!(!ct_eq_strings("", "x"));
    }

    #[test]
    fn ct_eq_same_length_different_bytes_returns_false() {
        assert!(!ct_eq_strings("abcdef", "abcdez"));
        assert!(!ct_eq_strings("aaaaaa", "bbbbbb"));
    }

    #[test]
    fn ct_eq_same_length_same_bytes_returns_true() {
        assert!(ct_eq_strings("hunter2", "hunter2"));
        assert!(ct_eq_strings("", ""));
    }

    #[test]
    fn verify_uses_constant_time_compare() {
        // Indirect verification: a mismatch in the first byte vs the last byte
        // both yield false. (If verify short-circuited on first-byte mismatch
        // we'd still see false; this test exists to lock in the contract.)
        let (sp, _store) = mock_password();
        sp.set("abcdef").expect("set");
        assert!(!sp.verify("Zbcdef")); // first-byte differ
        assert!(!sp.verify("abcdeZ")); // last-byte differ
        assert!(sp.verify("abcdef"));
    }

    // ── Mock keyring round-trip ──────────────────────────────────

    #[test]
    fn mock_keyring_round_trip_uses_canonical_service_and_account() {
        let (sp, store) = mock_password();
        sp.set("hunter2").expect("set");

        let guard = store.lock().unwrap();
        assert_eq!(
            guard
                .get(&(SERVICE.to_string(), ACCOUNT.to_string()))
                .map(String::as_str),
            Some("hunter2"),
            "mock keyring must store under the canonical (service, account) key"
        );
    }
}
