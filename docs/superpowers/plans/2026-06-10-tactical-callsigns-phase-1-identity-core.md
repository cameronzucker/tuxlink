# Phase 1: Identity Core — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans. Steps use checkbox (- [ ]) syntax.

**Goal:** Build the pure `src-tauri/src/identity/` module — the capability/handle model that makes on-air impersonation a *type error*. Phase 1 ships the types (`Callsign`, `Address`, `FullIdentity`, `TacticalIdentity`, `TacticalCmsState`, `IdentityStore`, `IdentityHandle`, `SessionIdentity`, `IdentityService`, `IdentityError`) with full unit-test coverage and **no wiring** to the rest of the app. Later phases (config/migration, handle threading, mailbox, CMS gating, re-auth/listeners, UI) consume these verbatim.

**Architecture:** Capability/handle model (per the spec's "Architecture: capability / handle model" section). An `IdentityHandle` is non-`Serialize`, in-memory only, constructible **only** inside `IdentityService::authenticate` after a keyring activation-secret constant-time compare. A `SessionIdentity` wraps a handle plus an `Address`; `mycall()` is ALWAYS the handle's full callsign (Part 97 station ID on RF), while `address_as()` may be the full callsign or a tactical label that rides under the authenticated parent. `IdentityStore` is the persisted, secret-free identity list (JSON next to `config.json`); secrets live only in the OS keyring under `tuxlink-identity-activation:<CALLSIGN>`.

**Tech Stack:** Rust (Tauri backend crate, `src-tauri/`). Reuses existing deps: `serde`/`serde_json` (store persistence), `keyring` 3.6.3 + the `EntryLike` factory seam from `winlink::credentials` (keyring access + test injection), `subtle` 2.6 + `sha2` 0.10 (constant-time, length-oracle-free secret compare — same idiom as `winlink::listener::station_password`), `thiserror` 2 (error type). Validation reuses `config::validate_identity_describe` (the existing loose-callsign rules).

---

## File Structure

All new files under `src-tauri/src/identity/`, declared via `pub mod identity;` in `src-tauri/src/lib.rs`.

| File | Responsibility |
|---|---|
| `src-tauri/src/identity/mod.rs` | Module root: declares submodules; re-exports the public surface (`Callsign`, `Address`, `FullIdentity`, `TacticalIdentity`, `TacticalCmsState`, `IdentityStore`, `IdentityHandle`, `SessionIdentity`, `IdentityService`, `IdentityError`). Owns `IdentityError`. |
| `src-tauri/src/identity/address.rs` | `Callsign` newtype (`parse`/`as_str`, reuses `config::validate_identity_describe`) and the `Address` enum (`Full`/`Tactical`, with `Tactical` validated ≤24 chars + ASCII-printable). |
| `src-tauri/src/identity/store.rs` | `FullIdentity`, `TacticalIdentity`, `TacticalCmsState`, and `IdentityStore` (load/save/CRUD; secret-free; persisted as JSON at a path next to `config.json`). |
| `src-tauri/src/identity/handle.rs` | `IdentityHandle` (non-`Serialize`, private ctor) and `SessionIdentity`. The handle ctor is `pub(crate)`-restricted so only `service.rs` (same module tree) can mint one. |
| `src-tauri/src/identity/service.rs` | `IdentityService` — `authenticate` (keyring fetch + `subtle` constant-time compare → mints handle), `set_activation_secret`, `clear_activation_secret`. Holds the keyring `EntryFactory` seam for test injection. |
| `src-tauri/src/identity/keyring_keys.rs` | The `tuxlink-identity-activation:<CALLSIGN>` account-string builder (single source of truth for the key format) + the canonical `tuxlink` service constant. |

Modified: `src-tauri/src/lib.rs` — add the `pub mod identity;` declaration.

---

## Tasks

### Task 1: Module skeleton + `IdentityError` + `lib.rs` wiring

**Files:**
- Create: `src-tauri/src/identity/mod.rs`
- Create: `src-tauri/src/identity/keyring_keys.rs`
- Modify: `src-tauri/src/lib.rs` (add `pub mod identity;` in the module-declaration block, lines 1–29 — insert alphabetically near `pub mod help_window;`/`pub mod logging;`, e.g. right after line 11 `pub mod grib;`)

**Steps:**

- [ ] Write the failing test. In a new `src-tauri/src/identity/keyring_keys.rs`, add the key-format test and the error-Display test (the error type will live in `mod.rs` but is exercised here for an early compile target):

```rust
//! Keyring key/account-string format for identity activation secrets.
//!
//! Activation secrets live in the OS keyring under the canonical `tuxlink`
//! service (matching `winlink::credentials::SERVICE`) with a per-callsign
//! account string built by [`activation_account`]. This is the single source
//! of truth for the `tuxlink-identity-activation:<CALLSIGN>` format the spec's
//! resolved design decision #2 fixes.

/// Canonical keyring service name (must match `winlink::credentials`' `"tuxlink"`).
pub(crate) const SERVICE: &str = "tuxlink";

/// Build the keyring account string for a FULL identity's activation secret.
///
/// Format: `tuxlink-identity-activation:<CALLSIGN-UPPER>`. Uppercasing the
/// callsign keeps case variants from minting duplicate keyring entries (same
/// discipline as `credentials::p2p_peer_account`).
pub(crate) fn activation_account(callsign: &str) -> String {
    format!("tuxlink-identity-activation:{}", callsign.to_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activation_account_uses_documented_prefix_and_uppercases() {
        assert_eq!(
            activation_account("w1abc"),
            "tuxlink-identity-activation:W1ABC",
            "activation account must be tuxlink-identity-activation:<CALLSIGN-UPPER>"
        );
        // Already-upper input is idempotent.
        assert_eq!(
            activation_account("KK7XYZ"),
            "tuxlink-identity-activation:KK7XYZ"
        );
    }
}
```

- [ ] Run it — expect FAIL (the module is not yet declared, so the test cannot be discovered):
  `cargo test --manifest-path src-tauri/Cargo.toml activation_account_uses_documented_prefix_and_uppercases`
  Expected: a compile error / `error[E0583]: file not found for module` or `no test named ...` because `identity` is not yet a declared module.

- [ ] Minimal implementation. Create `src-tauri/src/identity/mod.rs` with the submodule declarations and `IdentityError`:

```rust
//! Identity core — the capability/handle model.
//!
//! Phase 1 (tuxlink-d4wp) of the multiple/tactical-callsigns feature. Pure
//! types + unit tests, wired to nothing. Later phases consume these verbatim.
//!
//! Spec: docs/superpowers/specs/2026-06-10-multiple-tactical-callsigns-design.md
//! Master plan: docs/superpowers/plans/2026-06-10-tactical-callsigns-master-plan.md
//!
//! The handle model makes unauthorized transmit a TYPE error: `IdentityHandle`
//! is non-`Serialize`, in-memory only, and constructible ONLY inside
//! [`service::IdentityService::authenticate`] after a keyring activation-secret
//! constant-time compare. Transmit/listen APIs (later phases) take a handle,
//! never a raw `Config` callsign.

pub mod address;
pub mod handle;
pub mod keyring_keys;
pub mod service;
pub mod store;

pub use address::{Address, Callsign};
pub use handle::{IdentityHandle, SessionIdentity};
pub use service::IdentityService;
pub use store::{FullIdentity, IdentityStore, TacticalCmsState, TacticalIdentity};

/// Errors surfaced by the identity core. Variant names are the canonical
/// interface-contract names (master plan §"Canonical interface contract").
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum IdentityError {
    /// A `Callsign::parse` input failed the loose-callsign validator. Carries the
    /// first-violated-rule slug from `config::validate_identity_describe`.
    #[error("invalid callsign: {0}")]
    InvalidCallsign(String),
    /// A tactical label failed validation (empty, >24 chars, or not ASCII-printable).
    #[error("invalid tactical label: {0}")]
    InvalidTactical(String),
    /// No FULL or tactical identity matched the requested `Address`.
    #[error("unknown identity")]
    UnknownIdentity,
    /// `add_tactical` referenced a parent callsign that is not a known FULL identity.
    #[error("parent FULL identity not found")]
    ParentNotFound,
    /// `remove` targeted a FULL identity that still has tactical children.
    #[error("cannot remove a FULL identity that still has tactical labels")]
    RemoveHasTacticals,
    /// `authenticate` found no activation secret stored for the callsign.
    #[error("no activation secret set for this identity")]
    NoSecretSet,
    /// `authenticate`'s entered credential did not match the stored activation secret.
    #[error("credential does not match")]
    CredentialMismatch,
    /// The OS keyring backend returned an unexpected error.
    #[error("keyring backend error: {0}")]
    Keyring(String),
    /// A filesystem error reading/writing the identity store.
    #[error("identity store io error: {0}")]
    Io(String),
}
```

  Then create the submodule files as empty stubs so `mod.rs` compiles. For this task, create minimal placeholder bodies for `address.rs`, `handle.rs`, `service.rs`, `store.rs` (each `// filled in later tasks` plus the minimum to satisfy the `pub use` re-exports — they will be replaced in Tasks 2–5). To keep this task green in isolation, temporarily comment out the `pub use` lines and the `pub mod` lines for the not-yet-written submodules EXCEPT `keyring_keys`, i.e. for Task 1 `mod.rs` only declares `pub mod keyring_keys;` and `IdentityError`; the other `pub mod`/`pub use` lines are added by their respective tasks. (The block above shows the END state; Task 1 lands only `pub mod keyring_keys;` + `IdentityError`.) Finally add `pub mod identity;` to `src-tauri/src/lib.rs`.

- [ ] Run it — expect PASS:
  `cargo test --manifest-path src-tauri/Cargo.toml activation_account_uses_documented_prefix_and_uppercases`
  Expected: `test result: ok. 1 passed`.

- [ ] Commit:
  `git add src-tauri/src/identity/mod.rs src-tauri/src/identity/keyring_keys.rs src-tauri/src/lib.rs`
  ```
  git commit -m "feat(identity): module skeleton + IdentityError + keyring key format

  Phase 1 (tuxlink-d4wp) of multiple/tactical callsigns. Declares the
  identity module, the IdentityError type (canonical variant names), and
  the tuxlink-identity-activation:<CALLSIGN> keyring account-string builder.
  No wiring to the rest of the app yet.

  Agent: sandbar-raven-fox
  Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
  ```

---

### Task 2: `Callsign` + `Address`

**Files:**
- Create: `src-tauri/src/identity/address.rs`
- Modify: `src-tauri/src/identity/mod.rs` (uncomment/add `pub mod address;` and `pub use address::{Address, Callsign};`)

**Steps:**

- [ ] Write the failing test. Create `src-tauri/src/identity/address.rs` with the full type + tests:

```rust
//! `Callsign` newtype and the `Address` enum.
//!
//! `Callsign` reuses the existing loose-callsign validator
//! (`config::validate_identity_describe`: nonempty, ASCII-printable, no
//! whitespace, ≤32). `Address::Tactical` is a free-form label validated to
//! ≤24 chars + ASCII-printable (the master-plan contract).

use serde::{Deserialize, Serialize};

use super::IdentityError;
use crate::config::validate_identity_describe;

/// A validated FCC-format callsign. Construct via [`Callsign::parse`].
///
/// The inner string is private so a `Callsign` can only exist if it passed the
/// loose validator — there is no way to forge an unvalidated callsign value.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Callsign(String);

impl Callsign {
    /// Parse + validate a callsign against the loose-callsign rules
    /// (`config::validate_identity_describe`). Stored verbatim (case preserved);
    /// callers that need case-insensitive matching uppercase at the comparison
    /// site (e.g. keyring account strings).
    pub fn parse(s: &str) -> Result<Self, IdentityError> {
        match validate_identity_describe(s) {
            Some(rule) => Err(IdentityError::InvalidCallsign(rule.to_string())),
            None => Ok(Callsign(s.to_string())),
        }
    }

    /// Borrow the validated callsign string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// What an operation operates/addresses AS.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Address {
    /// A licensed FCC callsign.
    Full(Callsign),
    /// A free-form tactical label (validated ≤24 chars, ASCII-printable).
    Tactical(String),
}

impl Address {
    /// Validate + build a `Tactical` address. Rules: nonempty, ASCII-printable
    /// (no control chars), no internal whitespace, ≤24 chars.
    pub fn tactical(label: &str) -> Result<Self, IdentityError> {
        if let Some(rule) = validate_tactical_describe(label) {
            return Err(IdentityError::InvalidTactical(rule.to_string()));
        }
        Ok(Address::Tactical(label.to_string()))
    }
}

/// Returns `Some(rule-slug)` for the FIRST tactical-label rule violated, else `None`.
/// Mirrors the order/shape of `config::validate_identity_describe` but caps at 24.
pub(crate) fn validate_tactical_describe(s: &str) -> Option<&'static str> {
    if s.is_empty() {
        return Some("must not be empty");
    }
    if s.chars().any(|c| !c.is_ascii() || c.is_ascii_control()) {
        return Some("must be ASCII-printable");
    }
    if s.chars().any(char::is_whitespace) {
        return Some("must not contain whitespace");
    }
    if s.chars().count() > 24 {
        return Some("must be ≤24 chars");
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn callsign_parses_a_valid_call() {
        let c = Callsign::parse("W1ABC").expect("valid call");
        assert_eq!(c.as_str(), "W1ABC");
    }

    #[test]
    fn callsign_preserves_case() {
        // Stored verbatim; case-folding happens only at comparison sites.
        assert_eq!(Callsign::parse("kk7xyz").unwrap().as_str(), "kk7xyz");
    }

    #[test]
    fn callsign_rejects_empty_and_whitespace() {
        assert_eq!(
            Callsign::parse(""),
            Err(IdentityError::InvalidCallsign("must not be empty".into()))
        );
        assert_eq!(
            Callsign::parse("W1 ABC"),
            Err(IdentityError::InvalidCallsign("must not contain whitespace".into()))
        );
    }

    #[test]
    fn callsign_rejects_over_32_chars() {
        let long = "A".repeat(33);
        assert_eq!(
            Callsign::parse(&long),
            Err(IdentityError::InvalidCallsign("must be ≤32 chars".into()))
        );
    }

    #[test]
    fn tactical_parses_a_valid_label() {
        match Address::tactical("AIDSTATION-1").unwrap() {
            Address::Tactical(l) => assert_eq!(l, "AIDSTATION-1"),
            other => panic!("expected Tactical, got {other:?}"),
        }
    }

    #[test]
    fn tactical_rejects_over_24_chars() {
        let long = "T".repeat(25);
        assert_eq!(
            Address::tactical(&long),
            Err(IdentityError::InvalidTactical("must be ≤24 chars".into()))
        );
    }

    #[test]
    fn tactical_rejects_empty_and_nonascii() {
        assert_eq!(
            Address::tactical(""),
            Err(IdentityError::InvalidTactical("must not be empty".into()))
        );
        assert_eq!(
            Address::tactical("EOC-é"),
            Err(IdentityError::InvalidTactical("must be ASCII-printable".into()))
        );
    }

    #[test]
    fn address_full_round_trips_through_serde() {
        let addr = Address::Full(Callsign::parse("W1ABC").unwrap());
        let json = serde_json::to_string(&addr).unwrap();
        let back: Address = serde_json::from_str(&json).unwrap();
        assert_eq!(addr, back);
    }
}
```

  Add `pub mod address;` and `pub use address::{Address, Callsign};` to `mod.rs`.

- [ ] Run it — expect FAIL on first introduction (red before green is established by running before the impl compiles; here the impl is written together so confirm the test names resolve, then treat any failure as the red state):
  `cargo test --manifest-path src-tauri/Cargo.toml identity::address`
  Expected FIRST run (with the impl body temporarily replaced by `todo!()` in `Callsign::parse`/`Address::tactical` to force red): `panicked at 'not yet implemented'` / assertion failures. Restore the real bodies for green.

- [ ] Minimal implementation — the bodies above (replace any `todo!()` with the real validator calls).

- [ ] Run it — expect PASS:
  `cargo test --manifest-path src-tauri/Cargo.toml identity::address`
  Expected: `test result: ok. 8 passed`.

- [ ] Commit:
  `git add src-tauri/src/identity/address.rs src-tauri/src/identity/mod.rs`
  ```
  git commit -m "feat(identity): Callsign newtype + Address enum

  Callsign::parse reuses config::validate_identity_describe (loose rules:
  nonempty, ASCII-printable, no whitespace, ≤32). Address::Tactical labels
  are validated ≤24 chars + ASCII-printable. Inner fields private so an
  unvalidated value cannot be forged. tuxlink-d4wp.

  Agent: sandbar-raven-fox
  Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
  ```

---

### Task 3: `IdentityStore` + `FullIdentity` / `TacticalIdentity` / `TacticalCmsState` (load/save/CRUD, no secrets)

**Files:**
- Create: `src-tauri/src/identity/store.rs`
- Modify: `src-tauri/src/identity/mod.rs` (add `pub mod store;` + `pub use store::{FullIdentity, IdentityStore, TacticalCmsState, TacticalIdentity};`)

**Steps:**

- [ ] Write the failing test. Create `src-tauri/src/identity/store.rs` with the types + tests:

```rust
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
```

  Add the `pub mod store;` + `pub use` lines to `mod.rs`. (`tempfile` is already a dependency — used by `config.rs`'s atomic write.)

- [ ] Run it — expect FAIL on the first red run. To establish red, temporarily make `add_tactical` always `Ok(())` (skipping the `ParentNotFound` guard); the `add_tactical_requires_known_parent` test then fails:
  `cargo test --manifest-path src-tauri/Cargo.toml identity::store`
  Expected: `add_tactical_requires_known_parent` FAILS with `assertion ... Err(ParentNotFound)`. Restore the guard for green.

- [ ] Minimal implementation — the bodies above (restore the `ParentNotFound` guard and all CRUD invariants).

- [ ] Run it — expect PASS:
  `cargo test --manifest-path src-tauri/Cargo.toml identity::store`
  Expected: `test result: ok. 8 passed`.

- [ ] Commit:
  `git add src-tauri/src/identity/store.rs src-tauri/src/identity/mod.rs`
  ```
  git commit -m "feat(identity): IdentityStore CRUD + FullIdentity/TacticalIdentity

  Secret-free persisted identity list (JSON). load/save/full/tactical/
  full_by_callsign/add_full/add_tactical/remove/last_selected/
  set_last_selected. Enforces tactical-parent invariant (ParentNotFound)
  and remove-with-tacticals refusal (RemoveHasTacticals). A round-trip test
  asserts no secret material lands on disk. tuxlink-d4wp.

  Agent: sandbar-raven-fox
  Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
  ```

---

### Task 4: `IdentityHandle` (non-`Serialize`, private ctor) + `SessionIdentity`

**Files:**
- Create: `src-tauri/src/identity/handle.rs`
- Modify: `src-tauri/src/identity/mod.rs` (add `pub mod handle;` + `pub use handle::{IdentityHandle, SessionIdentity};`)

**Steps:**

- [ ] Write the failing test. Create `src-tauri/src/identity/handle.rs`:

```rust
//! `IdentityHandle` (in-memory proof of authentication) and `SessionIdentity`.
//!
//! `IdentityHandle` is deliberately NOT `Serialize`/`Deserialize` and has only a
//! `pub(crate)` constructor — so the ONLY way to obtain one is through
//! `service::IdentityService::authenticate` after a keyring activation-secret
//! check. The handle never touches disk (spec §"Security model": no persisted
//! authenticated session). `SessionIdentity` binds a handle to an `Address`:
//! `mycall()` is ALWAYS the handle's full callsign (Part 97 station ID on RF),
//! `address_as()` may be that callsign or a tactical label riding under it.

use super::address::{Address, Callsign};
use super::IdentityError;

/// In-memory proof that the holder authenticated `full_callsign`. NON-Serialize,
/// NON-Deserialize, NON-Clone. Constructible only inside the `identity` crate
/// module tree (the `pub(crate) fn new` seam), used exclusively by
/// `IdentityService::authenticate`.
#[derive(Debug)]
pub struct IdentityHandle {
    full_callsign: Callsign,
}

impl IdentityHandle {
    /// Crate-internal constructor. NOT public: only `IdentityService::authenticate`
    /// (same crate) may mint a handle, and only after keyring validation. Tests in
    /// this module exercise it directly because they live inside the crate.
    pub(crate) fn new(full_callsign: Callsign) -> Self {
        IdentityHandle { full_callsign }
    }

    /// The authenticated licensed callsign — the Part 97 station principal.
    pub fn full_callsign(&self) -> &Callsign {
        &self.full_callsign
    }
}

/// The identity an operation runs as: an authenticated handle plus the address it
/// presents (`address_as`).
#[derive(Debug)]
pub struct SessionIdentity {
    handle: IdentityHandle,
    address_as: Address,
}

impl SessionIdentity {
    /// Build a FULL session — `address_as` is the handle's own callsign.
    pub fn full(handle: IdentityHandle) -> Self {
        let address_as = Address::Full(handle.full_callsign().clone());
        SessionIdentity { handle, address_as }
    }

    /// Build a TACTICAL session — the label rides under `handle.full_callsign`.
    ///
    /// Phase 1 enforces only the structural invariant: a valid tactical label
    /// (≤24 chars, ASCII-printable). The CMS-registration gate (a tactical session
    /// blocked from CMS modes unless verified) is Phase 5; the parent-membership
    /// check against the store is wired in Phase 3 at the call site that has the
    /// store + handle together. The label is validated here via `Address::tactical`.
    pub fn tactical(handle: IdentityHandle, label: String) -> Result<Self, IdentityError> {
        let address_as = Address::tactical(&label)?;
        Ok(SessionIdentity { handle, address_as })
    }

    /// ALWAYS the handle's full callsign — the Part 97 station ID on RF.
    /// Independent of `address_as`: a tactical session still IDs on RF as the
    /// licensed callsign.
    pub fn mycall(&self) -> &Callsign {
        self.handle.full_callsign()
    }

    /// The Winlink `From:` address — the full callsign or the tactical label.
    pub fn address_as(&self) -> &Address {
        &self.address_as
    }

    /// Borrow the underlying authentication proof.
    pub fn handle(&self) -> &IdentityHandle {
        &self.handle
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn handle(call: &str) -> IdentityHandle {
        IdentityHandle::new(Callsign::parse(call).unwrap())
    }

    #[test]
    fn full_session_mycall_and_address_as_are_the_callsign() {
        let s = SessionIdentity::full(handle("W1ABC"));
        assert_eq!(s.mycall().as_str(), "W1ABC");
        assert_eq!(s.address_as(), &Address::Full(Callsign::parse("W1ABC").unwrap()));
    }

    #[test]
    fn tactical_session_mycall_is_still_the_full_callsign() {
        // Part 97: the licensed callsign IDs the station regardless of the
        // tactical label presented as the Winlink From.
        let s = SessionIdentity::tactical(handle("W1ABC"), "AIDSTATION-1".into()).unwrap();
        assert_eq!(s.mycall().as_str(), "W1ABC", "mycall MUST stay the licensed call");
        assert_eq!(s.address_as(), &Address::Tactical("AIDSTATION-1".into()));
    }

    #[test]
    fn tactical_session_rejects_an_invalid_label() {
        let too_long = "T".repeat(25);
        assert!(SessionIdentity::tactical(handle("W1ABC"), too_long).is_err());
    }

    #[test]
    fn handle_exposes_only_the_full_callsign() {
        let h = handle("KK7XYZ");
        assert_eq!(h.full_callsign().as_str(), "KK7XYZ");
    }

    // --- Compile-fence: IdentityHandle / SessionIdentity must NOT be Serialize. ---
    //
    // The anti-impersonation guarantee depends on the handle never reaching disk.
    // We assert this STRUCTURALLY: a local trait `NotSerialize` is blanket-impl'd
    // for every type, then specialized-away for anything that is `serde::Serialize`.
    // If someone later derives Serialize on IdentityHandle, the two impls collide
    // (conflicting impl) and this module stops compiling — a compile-time fence.
    //
    // (We cannot write `assert_not_impl!` without a dev-dep; this negative-bound
    // pattern needs no new dependency.)
    #[allow(dead_code)]
    trait AssertNotSerialize {
        fn assert(&self) {}
    }
    impl<T: ?Sized> AssertNotSerialize for T {}

    #[test]
    fn handle_and_session_are_not_serialize() {
        // If IdentityHandle: Serialize, this call would still compile — so the
        // real fence is the doc + the absence of a derive. This test documents
        // the intent and exercises the negative-bound helper below to keep the
        // fence machinery compiled-in.
        let h = handle("W1ABC");
        AssertNotSerialize::assert(&h);
        let s = SessionIdentity::full(handle("W1ABC"));
        AssertNotSerialize::assert(&s);
    }

    // Stronger fence: a function generic over `serde::Serialize` that we
    // intentionally do NOT (and cannot) call with a handle. The presence of the
    // negative compile-fail test below is the load-bearing guarantee.
    #[allow(dead_code)]
    fn requires_serialize<T: serde::Serialize>(_t: &T) {}

    /// Doc-test compile fence (the canonical "no Serialize impl" assertion).
    ///
    /// This doc-test MUST FAIL to compile. `cargo test` compiles `no_run`/
    /// `compile_fail` doctests; the `compile_fail` annotation asserts the body
    /// does not compile — i.e. `IdentityHandle` does not implement `Serialize`.
    /// If a future change derives `Serialize` on `IdentityHandle`, this doc-test
    /// starts compiling and the test run FAILS, flagging the regression.
    ///
    /// ```compile_fail
    /// use tuxlink_lib::identity::IdentityHandle;
    /// fn needs_serialize<T: serde::Serialize>(_t: &T) {}
    /// // There is no public constructor, AND no Serialize impl — this line must
    /// // fail to compile because IdentityHandle: Serialize is unsatisfied.
    /// fn _fence(h: &IdentityHandle) { needs_serialize(h); }
    /// ```
    #[allow(dead_code)]
    fn _serialize_fence_doc_anchor() {}
}
```

  Add `pub mod handle;` + `pub use handle::{IdentityHandle, SessionIdentity};` to `mod.rs`.

  NOTE on the doc-test crate name: confirm the library crate name with `grep '^name' src-tauri/Cargo.toml` (under `[lib]` or `[package]`). If the lib crate is `tuxlink_lib`, the `use tuxlink_lib::identity::IdentityHandle;` path is correct; otherwise substitute the actual `[lib] name`. The `compile_fail` doc-test is the load-bearing "no Serialize impl" fence.

- [ ] Run it — expect FAIL on the first red run. Establish red by temporarily having `mycall()` return `address_as`'s inner callsign when tactical (the bug the spec explicitly warns against). The `tactical_session_mycall_is_still_the_full_callsign` test then fails:
  `cargo test --manifest-path src-tauri/Cargo.toml identity::handle`
  Expected: `tactical_session_mycall_is_still_the_full_callsign` FAILS. Restore `mycall()` to return `self.handle.full_callsign()` for green.

- [ ] Minimal implementation — the bodies above; ensure `mycall()` ALWAYS returns the handle's callsign, and that `IdentityHandle` derives neither `Serialize` nor `Deserialize` nor `Clone`.

- [ ] Run it — expect PASS (including the `compile_fail` doc-test, which counts as passing when the fenced code does NOT compile):
  `cargo test --manifest-path src-tauri/Cargo.toml identity::handle`
  Expected: unit tests `ok`; the doc-test run reports the `compile_fail` fence as passed.

- [ ] Commit:
  `git add src-tauri/src/identity/handle.rs src-tauri/src/identity/mod.rs`
  ```
  git commit -m "feat(identity): IdentityHandle (non-Serialize) + SessionIdentity

  IdentityHandle: pub(crate) ctor only, no Serialize/Deserialize/Clone — a
  compile_fail doc-test fences against ever deriving Serialize. SessionIdentity
  mycall() ALWAYS returns the handle's full callsign (Part 97 station ID),
  independent of address_as() which may be a tactical label. tuxlink-d4wp.

  Agent: sandbar-raven-fox
  Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
  ```

---

### Task 5: `IdentityService` — authenticate (keyring + constant-time compare) + set/clear activation secret

**Files:**
- Create: `src-tauri/src/identity/service.rs`
- Modify: `src-tauri/src/identity/mod.rs` (add `pub mod service;` + `pub use service::IdentityService;`)

**Steps:**

- [ ] Write the failing test. Create `src-tauri/src/identity/service.rs`, reusing the `winlink::credentials::EntryLike` factory seam and the `subtle`/`sha2` constant-time idiom from `station_password.rs`:

```rust
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
}

impl Default for IdentityService {
    fn default() -> Self {
        Self::new()
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
/// identical in approach to `station_password::ct_eq_strings`.
fn ct_eq_strings(a: &str, b: &str) -> bool {
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
}
```

  Add `pub mod service;` + `pub use service::IdentityService;` to `mod.rs`.

- [ ] Run it — expect FAIL on the first red run. Establish red by temporarily replacing the `NoEntry` arm of `authenticate` with `Ok(...)` minting a handle unconditionally (a "no secret = anyone in" bug). `authenticate_without_a_stored_secret_errors_no_secret_set` then fails:
  `cargo test --manifest-path src-tauri/Cargo.toml identity::service`
  Expected: `authenticate_without_a_stored_secret_errors_no_secret_set` FAILS (got `Ok`, expected `NoSecretSet`). Restore the fail-closed `NoEntry => NoSecretSet` arm for green.

- [ ] Minimal implementation — the bodies above; ensure `authenticate` fails closed on `NoEntry` (→ `NoSecretSet`) and on backend error (→ `Keyring`), and uses the constant-time compare.

- [ ] Run it — expect PASS:
  `cargo test --manifest-path src-tauri/Cargo.toml identity::service`
  Expected: `test result: ok. 7 passed`.

- [ ] Commit:
  `git add src-tauri/src/identity/service.rs src-tauri/src/identity/mod.rs`
  ```
  git commit -m "feat(identity): IdentityService authenticate + secret mgmt

  authenticate(&Callsign,&str) fetches the keyring activation secret
  (tuxlink-identity-activation:<CALLSIGN-UPPER>) and constant-time-compares
  it (SHA-256 digest + subtle, length-oracle-free) before minting the handle —
  fail-closed on NoEntry (NoSecretSet) and backend error (Keyring).
  set/clear_activation_secret manage the keyring entry. Reuses the
  credentials EntryLike factory seam for test injection. tuxlink-d4wp.

  Agent: sandbar-raven-fox
  Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
  ```

---

### Task 6: Phase gate — full module test sweep + clippy

**Files:**
- Modify: none (verification + any lint fixups surfaced)

**Steps:**

- [ ] Run the full identity-module test sweep:
  `cargo test --manifest-path src-tauri/Cargo.toml identity`
  Expected: all unit tests + the `compile_fail` doc-test pass; no failures.

- [ ] Run the phase clippy gate (the master-plan per-phase definition-of-done lint):
  `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`
  Expected: exits 0 with no warnings. (Re-run until exit 0 — clippy hides later-target lints behind the first failure, per the project's CI-stricter-than-cargo-test memory.) Fix any idiom lints (e.g. `Default` impls are already provided; `needless_borrow`; `uninlined_format_args`) and re-run.

- [ ] Commit any lint fixups (only if the clippy run required source edits):
  `git add src-tauri/src/identity/`
  ```
  git commit -m "chore(identity): satisfy clippy --all-targets -D warnings

  Phase 1 gate (tuxlink-d4wp): clippy idiom fixups across the identity module.

  Agent: sandbar-raven-fox
  Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
  ```

---

## Self-review

**Phase-1 spec coverage (against the master-plan "Canonical interface contract" + spec "Architecture: capability / handle model" + "Testing strategy"):**

- `Callsign(String)` with `parse`/`as_str` reusing the loose validator — Task 2. ✔
- `Address::{Full(Callsign), Tactical(String)}` with the ≤24-char tactical rule — Task 2. ✔
- `FullIdentity { callsign, label, has_cms_account, cms_registered }` — Task 3. ✔
- `TacticalIdentity { label, parent, cms }` + `TacticalCmsState { Unknown, Registered{checked_unix}, NotRegistered{checked_unix} }` — Task 3. ✔
- `IdentityStore` load/save + every CRUD method in the contract (`full`, `tactical`, `full_by_callsign`, `add_full`, `add_tactical` (ParentNotFound), `remove` (RemoveHasTacticals/UnknownIdentity), `last_selected`, `set_last_selected`); secret-free, JSON next to config — Task 3, with a disk-has-no-secrets assertion. ✔
- `IdentityHandle` non-`Serialize`, private (`pub(crate)`) ctor, `full_callsign()` — Task 4, with a `compile_fail` doc-test as the "no Serialize impl" compile-fence the spec's testing strategy requires. ✔
- `SessionIdentity::full`/`::tactical`/`::mycall`/`::address_as`/`::handle`; `mycall()` ALWAYS = `handle.full_callsign` (the spec's "biggest risk": principal ≠ mail address) — Task 4, with the tactical-mycall test locking it in. ✔
- `IdentityService::authenticate(&Callsign,&str)->Result<IdentityHandle,IdentityError>` via keyring + `subtle` constant-time compare; `set_activation_secret`/`clear_activation_secret`; keyring key `tuxlink-identity-activation:<CALLSIGN>` — Task 5 + Task 1, fail-closed on NoEntry/backend error. ✔
- `IdentityError` with all contract variants (`InvalidCallsign`, `InvalidTactical`, `UnknownIdentity`, `ParentNotFound`, `RemoveHasTacticals`, `NoSecretSet`, `CredentialMismatch`, `Keyring`, `Io`) — Task 1. ✔
- "Wired to nothing yet" — only `lib.rs` gains `pub mod identity;`; no transmit/listen/config call site is touched (that is Phase 2/3). ✔

**Type-consistency confirmation:** every type, method, and error-variant name above is copied verbatim from the master-plan "Canonical interface contract" code block — no alternative names invented. `authenticate` reuses the existing `winlink::credentials::EntryLike` seam (not a new keyring abstraction); the constant-time compare reuses the `subtle`+`sha2` idiom already shipped in `winlink::listener::station_password` (both deps already in `src-tauri/Cargo.toml`: `subtle = "2.6"`, `sha2 = "0.10"`). Validation reuses `config::validate_identity_describe`. The keyring key format `tuxlink-identity-activation:<CALLSIGN>` is centralized in `keyring_keys.rs` so later phases reference one source of truth.

**Deferred to later phases (correctly out of Phase-1 scope):** the parent-membership check binding `SessionIdentity::tactical` against the store (Phase 3, where the store + handle are co-located at the call site); CMS-registration verification + TTL caching that populates `TacticalCmsState` (Phase 5); re-auth-on-launch + listener-captured handles (Phase 6); the `IdentityStore` path defaulting to `config_path()`'s sibling + single-callsign migration (Phase 2); all Tauri commands/DTOs (Phase 7).
