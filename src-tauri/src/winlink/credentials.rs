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
// P2P endpoint password helpers (Task 10 / VARA P2P peer model)
// ──────────────────────────────────────────────────────────────
//
// Endpoint passwords use `SERVICE` ("tuxlink") as the keyring service, but an
// id-keyed account string — `"p2p-endpoint:<peer_id>:<endpoint_id>"` — rather
// than the callsign-keyed `p2p-peer:<CALLSIGN>` account used above. Both
// `peer_id` and `endpoint_id` are stable, system-generated ids (Task 7
// model: Peer.id is a uuid v4, Endpoint.id a ULID) rather than
// operator/remote-supplied text, so the account string can never carry
// attacker-controlled bytes: the keyring account-string injection class is
// closed at the type level [R2-S10].
//
// Spec: docs/superpowers/specs/2026-07-10-p2p-peer-model-design.md
// [R2-S7][R1-C7][R5-5]

/// Build the keyring "account" string for a P2P endpoint password.
///
/// Keyed by ids, NOT by callsign [R2-S7][R1-C7]: ids are stable,
/// system-generated (no attacker-controlled bytes), so keyring
/// account-string injection is closed at the type level [R2-S10].
fn p2p_endpoint_account(peer_id: &str, endpoint_id: &str) -> String {
    format!("p2p-endpoint:{peer_id}:{endpoint_id}")
}

/// Read the password for a specific P2P endpoint from the keyring.
///
/// Returns `Ok(None)` on a keyring miss rather than an error — callers use
/// the `None` case to decide whether to attempt [`migrate_legacy_peer_secret`].
/// Accepts an entry factory for dependency injection.
pub fn p2p_endpoint_password_read_with_factory<F>(
    peer_id: &str,
    endpoint_id: &str,
    factory: &F,
) -> Result<Option<String>, String>
where
    F: Fn(&str, &str) -> Box<dyn EntryLike>,
{
    let account = p2p_endpoint_account(peer_id, endpoint_id);
    let entry = factory(SERVICE, &account);
    match entry.get_password() {
        Ok(password) => Ok(Some(password)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(other) => Err(format!("keyring backend error: {other}")),
    }
}

/// Read the password for a specific P2P endpoint from the OS keyring.
///
/// Returns `Ok(None)` on a keyring miss — this is not an error condition,
/// since the caller (Task 11/20) may attempt legacy migration on a miss.
pub fn p2p_endpoint_password_read(
    peer_id: &str,
    endpoint_id: &str,
) -> Result<Option<String>, String> {
    p2p_endpoint_password_read_with_factory(peer_id, endpoint_id, &real_entry_factory)
}

/// Write the password for a specific P2P endpoint to the keyring.
///
/// Overwrites any existing entry for this `(peer_id, endpoint_id)` pair.
/// Accepts an entry factory for dependency injection.
pub fn p2p_endpoint_password_write_with_factory<F>(
    peer_id: &str,
    endpoint_id: &str,
    password: &str,
    factory: &F,
) -> Result<(), String>
where
    F: Fn(&str, &str) -> Box<dyn EntryLike>,
{
    let account = p2p_endpoint_account(peer_id, endpoint_id);
    let entry = factory(SERVICE, &account);
    entry
        .set_password(password)
        .map_err(|e| format!("keyring backend error: {e}"))
}

/// Write the password for a specific P2P endpoint to the OS keyring.
pub fn p2p_endpoint_password_write(
    peer_id: &str,
    endpoint_id: &str,
    password: &str,
) -> Result<(), String> {
    p2p_endpoint_password_write_with_factory(peer_id, endpoint_id, password, &real_entry_factory)
}

/// Delete the password for a specific P2P endpoint from the keyring.
///
/// Idempotent: returns `Ok(())` if no entry exists. Accepts an entry factory
/// for dependency injection.
pub fn p2p_endpoint_password_delete_with_factory<F>(
    peer_id: &str,
    endpoint_id: &str,
    factory: &F,
) -> Result<(), String>
where
    F: Fn(&str, &str) -> Box<dyn EntryLike>,
{
    let account = p2p_endpoint_account(peer_id, endpoint_id);
    let entry = factory(SERVICE, &account);
    match entry.delete_password() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(other) => Err(format!("keyring backend error: {other}")),
    }
}

/// Delete the password for a specific P2P endpoint from the OS keyring.
///
/// Idempotent: returns `Ok(())` if no entry exists. Used by the peers-store
/// cascade clear (Task 8/20) so an endpoint or peer delete never orphans a
/// keyring secret.
pub fn p2p_endpoint_password_delete(peer_id: &str, endpoint_id: &str) -> Result<(), String> {
    p2p_endpoint_password_delete_with_factory(peer_id, endpoint_id, &real_entry_factory)
}

/// Outcome of a lazy legacy-secret migration attempt.
///
/// `Ambiguous` means the caller could not establish a unique mapping from the
/// legacy callsign-keyed secret to a single `(peer_id, endpoint_id)` pair; the
/// peers settings UI (Task 25) surfaces manual reassignment in that case.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LegacyMigration {
    /// The legacy secret was copied to the new id-keyed account and the
    /// legacy entry was deleted.
    Migrated,
    /// No legacy secret existed for this callsign; nothing to migrate.
    NoLegacySecret,
    /// The caller reported the mapping as ambiguous (`unambiguous == false`);
    /// no keyring mutation was attempted.
    Ambiguous,
}

/// Conservative legacy re-key [R5-5].
///
/// `unambiguous` is computed by the CALLER against the peers store: exactly
/// one peer with this base AND exactly one `Operator` endpoint. Anything else
/// must pass `unambiguous = false`, which short-circuits to `Ambiguous`
/// without touching the keyring — the peers settings UI (Task 25) then
/// surfaces manual reassignment.
///
/// The legacy entry is deleted strictly AFTER the new-key write succeeds —
/// there is no window where the secret exists in neither location.
///
/// A backend failure on that final legacy delete still returns
/// `Ok(Migrated)`: the migration functionally succeeded (the new key holds
/// the secret), and callers only attempt migration on a new-key read-miss,
/// so an `Err` here would report failure for a success and the delete would
/// never be re-attempted. The consequence is an ORPHANED legacy
/// `p2p-peer:<CALLSIGN>` keyring entry; a `tracing::warn!` names the
/// orphaned account (never the secret value) so the operator can remove it
/// manually. Uninstall cleanup still enumerates it via
/// `discover_peer_callsigns`.
///
/// Accepts an owned entry factory (matches the caller shape used by the
/// migration test double, which builds a fresh closure per call).
pub fn migrate_legacy_peer_secret_with_factory<F>(
    callsign: &str,
    peer_id: &str,
    endpoint_id: &str,
    unambiguous: bool,
    factory: F,
) -> Result<LegacyMigration, String>
where
    F: Fn(&str, &str) -> Box<dyn EntryLike>,
{
    if !unambiguous {
        return Ok(LegacyMigration::Ambiguous);
    }

    let legacy_account = p2p_peer_account(callsign);
    let legacy_entry = factory(SERVICE, &legacy_account);
    let password = match legacy_entry.get_password() {
        Ok(password) => password,
        Err(keyring::Error::NoEntry) => return Ok(LegacyMigration::NoLegacySecret),
        Err(other) => return Err(format!("keyring backend error: {other}")),
    };

    let new_account = p2p_endpoint_account(peer_id, endpoint_id);
    let new_entry = factory(SERVICE, &new_account);
    new_entry
        .set_password(&password)
        .map_err(|e| format!("keyring backend error: {e}"))?;

    // Delete-legacy happens strictly after the new-key write above succeeded.
    // A backend failure here does NOT fail the migration: the secret is
    // already safe under the new key, and this path is never re-entered
    // (callers migrate only on a new-key read-miss), so Err would both
    // misreport a success and guarantee the delete is never retried. Warn
    // with the orphaned ACCOUNT name only — never the secret value.
    match legacy_entry.delete_password() {
        Ok(()) | Err(keyring::Error::NoEntry) => {}
        Err(other) => {
            tracing::warn!(
                account = %legacy_account,
                error = %other,
                "legacy P2P peer secret migrated to the id-keyed account, but \
                 deleting the legacy keyring entry failed; the legacy entry is \
                 orphaned and can be removed manually"
            );
        }
    }
    Ok(LegacyMigration::Migrated)
}

/// Conservative legacy re-key of a callsign-keyed P2P peer secret into the
/// id-keyed `p2p-endpoint:<peer_id>:<endpoint_id>` account.
///
/// See [`migrate_legacy_peer_secret_with_factory`] for the full contract.
pub fn migrate_legacy_peer_secret(
    callsign: &str,
    peer_id: &str,
    endpoint_id: &str,
    unambiguous: bool,
) -> Result<LegacyMigration, String> {
    migrate_legacy_peer_secret_with_factory(
        callsign,
        peer_id,
        endpoint_id,
        unambiguous,
        real_entry_factory,
    )
}

// ──────────────────────────────────────────────────────────────
// Unit tests
// ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

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

    // ──────────────────────────────────────────────────────────
    // P2P endpoint keyring account + legacy migration (Task 10)
    // ──────────────────────────────────────────────────────────

    #[test]
    fn endpoint_account_is_id_keyed_not_callsign_keyed() {
        assert_eq!(
            p2p_endpoint_account("peer-uuid-1", "ep-uuid-2"),
            "p2p-endpoint:peer-uuid-1:ep-uuid-2"
        );
    }

    /// Fake keyring entry backed by a shared `HashMap<(service, account), password>`.
    ///
    /// Mirrors the `MockEntry` double in `winlink_credentials_test.rs`: the
    /// `keyring` crate's own mock builder cannot share state across separate
    /// `Entry::new` calls, so the migration test (read legacy → write new →
    /// delete legacy, three distinct accounts) needs a HashMap-backed double
    /// instead.
    struct FakeEntry {
        store: Arc<Mutex<HashMap<(String, String), String>>>,
        service: String,
        account: String,
    }

    impl EntryLike for FakeEntry {
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

    /// Build a factory closure backed by the given shared store.
    fn fake_factory(
        store: Arc<Mutex<HashMap<(String, String), String>>>,
    ) -> impl Fn(&str, &str) -> Box<dyn EntryLike> {
        move |service: &str, account: &str| -> Box<dyn EntryLike> {
            Box::new(FakeEntry {
                store: Arc::clone(&store),
                service: service.to_string(),
                account: account.to_string(),
            })
        }
    }

    #[test]
    fn migration_copies_then_deletes_legacy_only_after_write_success() {
        // Fake keyring: legacy secret exists; new key empty.
        let store = Arc::new(Mutex::new(HashMap::from([(
            ("tuxlink".to_string(), "p2p-peer:W6ABC".to_string()),
            "hunter2".to_string(),
        )])));
        let out = migrate_legacy_peer_secret_with_factory(
            "W6ABC",
            "p1",
            "e1",
            true,
            fake_factory(store.clone()),
        )
        .unwrap();
        assert_eq!(out, LegacyMigration::Migrated);
        let map = store.lock().unwrap();
        assert_eq!(
            map.get(&("tuxlink".into(), "p2p-endpoint:p1:e1".into()))
                .map(String::as_str),
            Some("hunter2")
        );
        assert!(
            !map.contains_key(&("tuxlink".into(), "p2p-peer:W6ABC".into())),
            "legacy deleted after write"
        );
    }

    #[test]
    fn ambiguous_mapping_migrates_nothing() {
        let store = Arc::new(Mutex::new(HashMap::from([(
            ("tuxlink".to_string(), "p2p-peer:W6ABC".to_string()),
            "hunter2".to_string(),
        )])));
        let out = migrate_legacy_peer_secret_with_factory(
            "W6ABC",
            "p1",
            "e1",
            false,
            fake_factory(store.clone()),
        )
        .unwrap();
        assert_eq!(out, LegacyMigration::Ambiguous);
        let map = store.lock().unwrap();
        assert!(
            map.contains_key(&("tuxlink".into(), "p2p-peer:W6ABC".into())),
            "legacy untouched"
        );
        assert!(!map.contains_key(&("tuxlink".into(), "p2p-endpoint:p1:e1".into())));
    }

    /// Like `FakeEntry`, but `delete_password` always fails with a backend
    /// error (NOT `NoEntry`). Models a keyring whose store is readable and
    /// writable but rejects deletions — the orphaned-legacy-entry scenario.
    struct DeleteFailEntry(FakeEntry);

    impl EntryLike for DeleteFailEntry {
        fn get_password(&self) -> Result<String, keyring::Error> {
            self.0.get_password()
        }

        fn set_password(&self, password: &str) -> Result<(), keyring::Error> {
            self.0.set_password(password)
        }

        fn delete_password(&self) -> Result<(), keyring::Error> {
            Err(keyring::Error::PlatformFailure(
                "simulated delete failure".into(),
            ))
        }
    }

    #[test]
    fn legacy_delete_failure_after_successful_write_still_reports_migrated() {
        // Reviewer fix: if the new-key write succeeds but the legacy delete
        // fails with a backend error, the migration DID succeed — callers
        // only migrate on a new-key read-miss, so an Err here would report
        // failure for a success and the delete would never be retried. The
        // legacy entry is orphaned (and warned about), not an error.
        let store = Arc::new(Mutex::new(HashMap::from([(
            ("tuxlink".to_string(), "p2p-peer:W6ABC".to_string()),
            "hunter2".to_string(),
        )])));
        let delete_fail_factory = {
            let store = store.clone();
            move |service: &str, account: &str| -> Box<dyn EntryLike> {
                Box::new(DeleteFailEntry(FakeEntry {
                    store: Arc::clone(&store),
                    service: service.to_string(),
                    account: account.to_string(),
                }))
            }
        };
        let out =
            migrate_legacy_peer_secret_with_factory("W6ABC", "p1", "e1", true, delete_fail_factory)
                .unwrap();
        assert_eq!(out, LegacyMigration::Migrated);
        let map = store.lock().unwrap();
        assert_eq!(
            map.get(&("tuxlink".into(), "p2p-endpoint:p1:e1".into()))
                .map(String::as_str),
            Some("hunter2"),
            "new key holds the migrated secret"
        );
        assert!(
            map.contains_key(&("tuxlink".into(), "p2p-peer:W6ABC".into())),
            "legacy entry remains (orphaned) because its delete failed"
        );
    }

    #[test]
    fn no_legacy_secret_when_neither_present() {
        // Self-review edge case: unambiguous mapping, but no legacy secret at
        // all (a fresh peer that never had a callsign-keyed secret) →
        // NoLegacySecret, not a Backend error.
        let store: Arc<Mutex<HashMap<(String, String), String>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let out = migrate_legacy_peer_secret_with_factory(
            "W6ABC",
            "p1",
            "e1",
            true,
            fake_factory(store.clone()),
        )
        .unwrap();
        assert_eq!(out, LegacyMigration::NoLegacySecret);
        assert!(store.lock().unwrap().is_empty());
    }

    #[test]
    fn endpoint_password_roundtrip_and_idempotent_delete() {
        // Self-review: the plain read/write/delete surface Tasks 11/20 consume
        // directly, not just the migration path.
        let store: Arc<Mutex<HashMap<(String, String), String>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let factory = fake_factory(store.clone());

        assert_eq!(
            p2p_endpoint_password_read_with_factory("p1", "e1", &factory).unwrap(),
            None
        );
        p2p_endpoint_password_write_with_factory("p1", "e1", "s3cret", &factory).unwrap();
        assert_eq!(
            p2p_endpoint_password_read_with_factory("p1", "e1", &factory).unwrap(),
            Some("s3cret".to_string())
        );
        p2p_endpoint_password_delete_with_factory("p1", "e1", &factory).unwrap();
        assert_eq!(
            p2p_endpoint_password_read_with_factory("p1", "e1", &factory).unwrap(),
            None
        );
        // Idempotent: deleting an already-absent entry is still Ok(()).
        p2p_endpoint_password_delete_with_factory("p1", "e1", &factory).unwrap();
    }
}
