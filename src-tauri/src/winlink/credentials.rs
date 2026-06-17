//! Keyring credential read helper.
//!
//! All tuxlink credentials live under the `"tuxlink"` keyring service. `read_password`
//! reads from `"tuxlink"`; there is no fallback. (The Pat-era `"tuxlink-pat"` service and
//! its one-time migration fallback were removed in tuxlink-kc3q — Pat is fully stripped.)
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
/// - `KeyringError::NoEntry` — no `"tuxlink"` entry exists for this callsign.
/// - `KeyringError::Backend` — an unexpected error from the underlying credential store.
pub fn read_password_with_factory<F>(callsign: &str, factory: &F) -> Result<String, KeyringError>
where
    F: Fn(&str, &str) -> Box<dyn EntryLike>,
{
    // Read the canonical service. No legacy fallback — Pat is fully stripped (tuxlink-kc3q).
    let entry = factory(SERVICE, callsign);
    match entry.get_password() {
        Ok(password) => Ok(password),
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
/// Reads from the `"tuxlink"` service (no legacy fallback — Pat fully stripped).
///
/// # Errors
///
/// - `KeyringError::NoEntry` — no entry for this callsign.
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
        .map_err(|e| KeyringError::Backend(format!("{e}")))?;

    // tuxlink-6wz3: keep the identity activation secret in lockstep with the CMS
    // credential. Both callers of write_password (the first-run wizard's
    // persist_cms_impl and the credentials_write_password command) write the
    // operator's OWN identity credential, so the active-identity gate + launch
    // auto-auth authenticate against the exact value the operator just set —
    // the two keyring entries can never drift. Guarded by Callsign::parse so a
    // non-callsign account (none exists today) is skipped rather than minting a
    // spurious activation secret.
    if let Ok(cs) = crate::identity::Callsign::parse(callsign) {
        crate::identity::IdentityService::new()
            .set_activation_secret(&cs, password)
            .map_err(|e| KeyringError::Backend(format!("activation-secret sync: {e}")))?;
    }
    Ok(())
}

/// Delete the Winlink password for `callsign` from the OS keyring.
///
/// Used when a CMS account is removed at the server (tuxlink-vfb3 `account_remove`)
/// so the now-dead stored credential is dropped. Idempotent: a missing entry
/// (`NoEntry`) is success. Any other backend error is surfaced so the caller can
/// report a keyring/CMS desync rather than a false success.
pub fn delete_password(callsign: &str) -> Result<(), KeyringError> {
    let entry = keyring::Entry::new(SERVICE, callsign)
        .expect("keyring::Entry::new should not fail for valid service/account strings");
    let entry = RealEntry(entry);
    match entry.delete_password() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(other) => Err(KeyringError::Backend(format!("{other}"))),
    }
}

// ──────────────────────────────────────────────────────────────
// Winlink station-listing service codes (tuxlink-6j14)
// ──────────────────────────────────────────────────────────────
//
// A service code is a sysop-assigned TAG that filters which gateways the
// listing endpoint returns (`…/listings/<Mode>Listing.aspx?serviceCodes=X`).
// It is a client-side DIRECTORY FILTER only — never sent at connect time, never
// a connection credential. PUBLIC and EMCOMM are the only publicly-blessed
// codes; group codes (MARS/SHARES) are member-issued FOUO secrets. The operator
// supplies their own; tuxlink hardcodes NONE and stores whatever they enter in
// the OS keyring rather than plaintext config, so a FOUO code is not left on
// disk. (WLE ships these as plaintext constants in the binary — the failure
// mode this deliberately avoids.)

/// Keyring account string for the configured service codes. Per-installation
/// (one tagging policy per station), distinct from per-callsign credentials.
const SERVICE_CODES_ACCOUNT: &str = "catalog-service-codes";

/// The default when nothing is configured: the public amateur gateway set.
pub const DEFAULT_SERVICE_CODES: &str = "PUBLIC";

/// Normalize an operator-entered service-code string: trim, collapse internal
/// whitespace to single spaces (Winlink lists multiple codes space-separated),
/// and fall back to [`DEFAULT_SERVICE_CODES`] when the result is empty. Case is
/// preserved — FOUO codes may be case-sensitive.
pub fn normalize_service_codes(input: &str) -> String {
    let joined = input.split_whitespace().collect::<Vec<_>>().join(" ");
    if joined.is_empty() {
        DEFAULT_SERVICE_CODES.to_string()
    } else {
        joined
    }
}

/// Read the configured service codes, with an injected entry factory.
///
/// Always returns a usable, non-empty code string: any keyring miss or backend
/// error (no entry yet, no keyring daemon, locked store) degrades cleanly to
/// [`DEFAULT_SERVICE_CODES`] so station fetching keeps working everywhere
/// (including CI and headless shells with no secret service).
pub fn service_codes_read_with_factory<F>(factory: &F) -> String
where
    F: Fn(&str, &str) -> Box<dyn EntryLike>,
{
    let entry = factory(SERVICE, SERVICE_CODES_ACCOUNT);
    match entry.get_password() {
        Ok(value) => normalize_service_codes(&value),
        Err(_) => DEFAULT_SERVICE_CODES.to_string(),
    }
}

/// Read the configured service codes from the OS keyring, defaulting to
/// [`DEFAULT_SERVICE_CODES`] on any miss/error. Never panics, never errors.
pub fn service_codes_read() -> String {
    service_codes_read_with_factory(&real_entry_factory)
}

/// Write service codes to the keyring (normalized first), with an injected factory.
///
/// # Errors
///
/// - `KeyringError::Backend` — the keyring backend returned an error on write.
pub fn service_codes_write_with_factory<F>(codes: &str, factory: &F) -> Result<(), KeyringError>
where
    F: Fn(&str, &str) -> Box<dyn EntryLike>,
{
    let normalized = normalize_service_codes(codes);
    let entry = factory(SERVICE, SERVICE_CODES_ACCOUNT);
    entry
        .set_password(&normalized)
        .map_err(|e| KeyringError::Backend(format!("{e}")))
}

/// Write the configured service codes to the OS keyring.
///
/// # Errors
///
/// - `KeyringError::Backend` — unexpected backend error.
pub fn service_codes_write(codes: &str) -> Result<(), KeyringError> {
    service_codes_write_with_factory(codes, &real_entry_factory)
}

/// The production keyring entry factory (wraps `keyring::Entry::new`).
fn real_entry_factory(service: &str, account: &str) -> Box<dyn EntryLike> {
    let entry = keyring::Entry::new(service, account)
        .expect("keyring::Entry::new should not fail for valid service/account strings");
    Box::new(RealEntry(entry))
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
