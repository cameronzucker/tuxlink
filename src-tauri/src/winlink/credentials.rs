//! Keyring credential read + one-time migration helper.
//!
//! Service name history:
//! - `"tuxlink-pat"` — used during the Pat era; the service name reflected the Pat sidecar.
//! - `"tuxlink"` — canonical name post-Pat-strip. This is what new entries use.
//!
//! `read_password` reads from `"tuxlink"` first. If no entry exists, it falls back to
//! `"tuxlink-pat"` (one-time migration): on success it writes the password to the new
//! entry and best-effort deletes the old one, then returns the password. This is
//! transparent to operators who set their password during the Pat era.
//!
//! # Test isolation
//!
//! The `read_password` public API is sugar over `read_password_with_factory`, which
//! accepts an `EntryLike` factory closure for dependency injection. Tests use a
//! `HashMap`-backed mock factory that shares state across calls for the same
//! `(service, account)` — something `keyring::mock`'s `EntryOnly` persistence
//! cannot provide. Production uses `keyring::Entry::new` directly.
//!
//! See `src-tauri/tests/winlink_credentials_test.rs` for the full test suite.
//! See `docs/pitfalls/testing-pitfalls.md §7` for the keyring isolation contract.
//!
//! Spec: docs/superpowers/specs/2026-05-30-strip-pat-add-native-attachments.md §7.1
//! Plan: Phase 7 Task 7.1 / tuxlink-9phd

/// The canonical keyring service name for tuxlink credentials.
const SERVICE: &str = "tuxlink";
/// Legacy service name from the Pat era. Used only during one-time migration.
const LEGACY_SERVICE: &str = "tuxlink-pat";

// ──────────────────────────────────────────────────────────────
// Public error type
// ──────────────────────────────────────────────────────────────

/// Error returned by `read_password` (and its test-mockable sibling).
#[derive(Debug, PartialEq)]
pub enum KeyringError {
    /// No password found for this callsign in either the new or legacy service.
    NoEntry { callsign: String },
    /// The underlying keyring backend returned an unexpected error.
    Backend(String),
}

impl std::fmt::Display for KeyringError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KeyringError::NoEntry { callsign } => {
                write!(f, "no keyring entry for callsign {callsign}")
            }
            KeyringError::Backend(msg) => write!(f, "keyring backend error: {msg}"),
        }
    }
}

impl std::error::Error for KeyringError {}

// ──────────────────────────────────────────────────────────────
// EntryLike trait — the abstraction injected by tests
// ──────────────────────────────────────────────────────────────

/// A minimal abstraction over a keyring entry.
///
/// Production: backed by `keyring::Entry`.
/// Tests: backed by `MockEntry` (see `winlink_credentials_test.rs`).
pub trait EntryLike {
    fn get_password(&self) -> Result<String, keyring::Error>;
    fn set_password(&self, password: &str) -> Result<(), keyring::Error>;
    fn delete_password(&self) -> Result<(), keyring::Error>;
}

// ──────────────────────────────────────────────────────────────
// Production Entry wrapper
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
// Internal implementation
// ──────────────────────────────────────────────────────────────

/// Internal implementation accepting an entry factory for testability.
///
/// The `factory` closure is called as `factory(service, account)` and must
/// return a boxed `EntryLike`. In production this wraps `keyring::Entry::new`;
/// in tests it wraps an in-memory mock.
///
/// # Errors
///
/// - `KeyringError::NoEntry` — neither the `"tuxlink"` nor the `"tuxlink-pat"` entry
///   exists for this callsign.
/// - `KeyringError::Backend` — an unexpected error from the underlying credential store.
pub fn read_password_with_factory<F>(callsign: &str, factory: &F) -> Result<String, KeyringError>
where
    F: Fn(&str, &str) -> Box<dyn EntryLike>,
{
    // Step 1: try the canonical service name.
    let new_entry = factory(SERVICE, callsign);
    match new_entry.get_password() {
        Ok(password) => return Ok(password),
        Err(keyring::Error::NoEntry) => {} // fall through to migration
        Err(other) => return Err(KeyringError::Backend(format!("{other}"))),
    }

    // Step 2: canonical entry absent — try the legacy service (one-time migration).
    let old_entry = factory(LEGACY_SERVICE, callsign);
    match old_entry.get_password() {
        Ok(password) => {
            // Migrate: write to new service name.
            let migrate_entry = factory(SERVICE, callsign);
            if let Err(e) = migrate_entry.set_password(&password) {
                // Migration write failed — still return the password; next call retries.
                eprintln!(
                    "credentials: migration write to '{}' failed for {callsign}: {e}",
                    SERVICE
                );
            } else {
                // Best-effort delete from legacy service.
                if let Err(e) = old_entry.delete_password() {
                    eprintln!(
                        "credentials: best-effort delete from '{}' failed for {callsign}: {e}",
                        LEGACY_SERVICE
                    );
                }
                eprintln!(
                    "credentials: migrated {callsign} from '{}' to '{}'",
                    LEGACY_SERVICE, SERVICE
                );
            }
            Ok(password)
        }
        Err(keyring::Error::NoEntry) => Err(KeyringError::NoEntry {
            callsign: callsign.to_string(),
        }),
        Err(other) => Err(KeyringError::Backend(format!("{other}"))),
    }
}

// ──────────────────────────────────────────────────────────────
// Public API
// ──────────────────────────────────────────────────────────────

/// Read the Winlink password for `callsign` from the OS keyring.
///
/// Reads from the `"tuxlink"` service. If absent, transparently migrates from the
/// legacy `"tuxlink-pat"` service (one-time, logged at `info`).
///
/// # Errors
///
/// - `KeyringError::NoEntry` — no entry in either service.
/// - `KeyringError::Backend` — unexpected backend error.
pub fn read_password(callsign: &str) -> Result<String, KeyringError> {
    let real_factory = |service: &str, account: &str| -> Box<dyn EntryLike> {
        let entry = keyring::Entry::new(service, account)
            .expect("keyring::Entry::new should not fail for valid service/account strings");
        Box::new(RealEntry(entry))
    };
    read_password_with_factory(callsign, &real_factory)
}

// ──────────────────────────────────────────────────────────────
// P2P peer password helpers
// ──────────────────────────────────────────────────────────────
//
// Peer passwords use `SERVICE` ("tuxlink") as the keyring service, but a
// `"p2p-peer:<CALLSIGN-UPPER>"` account string so they live in a distinct
// namespace from the CMS-secure-login credentials (which use just the callsign
// as the account).
//
// Spec: docs/design/2026-06-01-tcp-p2p-telnet-design.md §4.4
// Plan: 2026-06-01-tcp-p2p-telnet-pr1-client-dial.md Task 1 (tuxlink-0pnb)

/// Build the keyring "account" string for a P2P peer password.
///
/// Uppercases the callsign so case variants don't create duplicate entries.
fn p2p_peer_account(callsign: &str) -> String {
    format!("p2p-peer:{}", callsign.to_uppercase())
}

/// Read the password for a specific P2P peer from the keyring.
///
/// Returns `KeyringError::NoEntry { callsign }` if no entry exists.
/// Uses `SERVICE` ("tuxlink") as the keyring service name.
///
/// Accepts an entry factory for dependency injection; production callers use
/// `p2p_peer_password_read` which supplies the real `keyring::Entry` factory.
pub fn p2p_peer_password_read_with_factory<F>(
    callsign: &str,
    factory: &F,
) -> Result<String, KeyringError>
where
    F: Fn(&str, &str) -> Box<dyn EntryLike>,
{
    let account = p2p_peer_account(callsign);
    let entry = factory(SERVICE, &account);
    match entry.get_password() {
        Ok(password) => Ok(password),
        Err(keyring::Error::NoEntry) => Err(KeyringError::NoEntry {
            callsign: callsign.to_string(),
        }),
        Err(other) => Err(KeyringError::Backend(format!("{other}"))),
    }
}

/// Read the password for a specific P2P peer from the OS keyring.
///
/// # Errors
///
/// - `KeyringError::NoEntry` — no entry found for this peer callsign.
/// - `KeyringError::Backend` — unexpected backend error.
pub fn p2p_peer_password_read(callsign: &str) -> Result<String, KeyringError> {
    let real_factory = |service: &str, account: &str| -> Box<dyn EntryLike> {
        let entry = keyring::Entry::new(service, account)
            .expect("keyring::Entry::new should not fail for valid service/account strings");
        Box::new(RealEntry(entry))
    };
    p2p_peer_password_read_with_factory(callsign, &real_factory)
}

/// Write the password for a specific P2P peer to the keyring.
///
/// Overwrites any existing entry for this peer callsign.
/// Accepts an entry factory for dependency injection.
pub fn p2p_peer_password_write_with_factory<F>(
    callsign: &str,
    password: &str,
    factory: &F,
) -> Result<(), KeyringError>
where
    F: Fn(&str, &str) -> Box<dyn EntryLike>,
{
    let account = p2p_peer_account(callsign);
    let entry = factory(SERVICE, &account);
    entry
        .set_password(password)
        .map_err(|e| KeyringError::Backend(format!("{e}")))
}

/// Write the password for a specific P2P peer to the OS keyring.
///
/// # Errors
///
/// - `KeyringError::Backend` — unexpected backend error.
pub fn p2p_peer_password_write(callsign: &str, password: &str) -> Result<(), KeyringError> {
    let real_factory = |service: &str, account: &str| -> Box<dyn EntryLike> {
        let entry = keyring::Entry::new(service, account)
            .expect("keyring::Entry::new should not fail for valid service/account strings");
        Box::new(RealEntry(entry))
    };
    p2p_peer_password_write_with_factory(callsign, password, &real_factory)
}

/// Delete the password for a specific P2P peer from the keyring.
///
/// Idempotent: returns `Ok(())` if no entry exists.
/// Accepts an entry factory for dependency injection.
pub fn p2p_peer_password_delete_with_factory<F>(
    callsign: &str,
    factory: &F,
) -> Result<(), KeyringError>
where
    F: Fn(&str, &str) -> Box<dyn EntryLike>,
{
    let account = p2p_peer_account(callsign);
    let entry = factory(SERVICE, &account);
    match entry.delete_password() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()), // idempotent
        Err(other) => Err(KeyringError::Backend(format!("{other}"))),
    }
}

/// Delete the password for a specific P2P peer from the OS keyring.
///
/// Idempotent: returns `Ok(())` if no entry exists.
///
/// # Errors
///
/// - `KeyringError::Backend` — unexpected backend error.
pub fn p2p_peer_password_delete(callsign: &str) -> Result<(), KeyringError> {
    let real_factory = |service: &str, account: &str| -> Box<dyn EntryLike> {
        let entry = keyring::Entry::new(service, account)
            .expect("keyring::Entry::new should not fail for valid service/account strings");
        Box::new(RealEntry(entry))
    };
    p2p_peer_password_delete_with_factory(callsign, &real_factory)
}

// ──────────────────────────────────────────────────────────────
// CMS credential write
// ──────────────────────────────────────────────────────────────

/// Write a Winlink password for `callsign` to the OS keyring.
///
/// Preserves the read-first → set_password destructive-overwrite-readback
/// discipline from `wizard.rs` (spec §3.2): the existing entry is read before
/// `set_password` is called so that a backend error on read aborts early, before
/// any destructive overwrite occurs.
///
/// This is the public seam used by:
/// - the `credentials_write_password` Tauri command (Task 13, §4.3 (i))
/// - the inline re-enter-password flow in the auth-diagnostics banner (Task 21)
///
/// R5 adrev R2 #4: the prior spec assumed a `credentials::set_password` API that
/// did not exist; this function creates the correct public surface.
///
/// The wizard (`persist_cms_impl`) is refactored to delegate its keyring write
/// (step 6) to this function while retaining its own snapshot read (step 5) for
/// the config-write rollback path.
///
/// # Errors
///
/// - `KeyringError::Backend` — the underlying keyring backend returned an error
///   on either the read-first probe or the write.
pub fn write_password(callsign: &str, password: &str) -> Result<(), KeyringError> {
    let entry = keyring::Entry::new(SERVICE, callsign)
        .expect("keyring::Entry::new should not fail for valid service/account strings");
    let entry = RealEntry(entry);

    // Read-first: if the backend is unavailable / locked, abort BEFORE the
    // destructive set_password so callers see the error without a partial write.
    // `NoEntry` is fine — it just means we're creating a fresh credential.
    match entry.get_password() {
        Ok(_) | Err(keyring::Error::NoEntry) => {}
        Err(other) => return Err(KeyringError::Backend(format!("{other}"))),
    }

    entry
        .set_password(password)
        .map_err(|e| KeyringError::Backend(format!("{e}")))
}

// ──────────────────────────────────────────────────────────────
// Unit tests
// ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Compile-time check that `write_password` exists with the documented
    /// signature. Behavioral correctness is covered by the keyring mock tests
    /// in `src-tauri/tests/winlink_credentials_test.rs`.
    #[test]
    fn write_password_signature_compiles() {
        // We call the function but don't assert Ok/Err because the OS keyring
        // state depends on the test environment (mock backend vs. real secretsd).
        // The goal is that this compiles and the signature matches the seam
        // expected by Task 13 (credentials_write_password Tauri command).
        let result: Result<(), KeyringError> = write_password("TEST-CALL", "test-password");
        let _ = result;
    }
}
