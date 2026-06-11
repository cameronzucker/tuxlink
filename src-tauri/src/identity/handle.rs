//! `IdentityHandle` (in-memory proof of authentication) and `SessionIdentity`.
//!
//! `IdentityHandle` is deliberately NOT `Serialize`/`Deserialize` and has only a
//! `pub(crate)` constructor — so the ONLY way to obtain one in production is
//! through `service::IdentityService::authenticate` after a keyring
//! activation-secret check. The handle never touches disk (spec §"Security
//! model": no persisted authenticated session). It IS `Clone` (cheap,
//! `Arc`-backed): per the master plan's cross-phase reconciliation #1, Phase 6
//! armed listeners capture their own handle at arm time while the active session
//! co-holds one — the no-persist guarantee comes from the absence of a
//! `Serialize` impl + never writing the handle to disk, NOT from non-`Clone`.
//!
//! `SessionIdentity` binds a handle to an `Address`: `mycall()` is ALWAYS the
//! handle's full callsign (Part 97 station ID on RF), `address_as()` may be that
//! callsign or a tactical label riding under it.

use std::sync::Arc;

use super::address::{Address, Callsign};
use super::IdentityError;

/// Private inner of [`IdentityHandle`], shared behind an `Arc` so clones are cheap
/// and all clones observe the same authenticated callsign.
#[derive(Debug)]
struct HandleInner {
    full_callsign: Callsign,
}

/// In-memory proof that the holder authenticated `full_callsign`. NON-`Serialize`,
/// NON-`Deserialize`. `Clone` is intentionally derived (cheap — it bumps the inner
/// `Arc`): armed listeners (Phase 6) capture their own handle while the active
/// session also holds one. Constructible only inside the `identity` module tree
/// via the `pub(crate)` [`IdentityHandle::new`] seam (used by
/// `IdentityService::authenticate`) or, in tests, [`IdentityHandle::for_test`].
///
/// # Compile-fence: `IdentityHandle` must never be `Serialize`
///
/// The anti-impersonation guarantee depends on the handle never reaching disk.
/// The doc-test below asserts there is no `Serialize` impl — it MUST fail to
/// compile. If a future change derives `Serialize` on `IdentityHandle`, the
/// doc-test starts compiling and `cargo test --doc` FAILS, flagging the
/// regression.
///
/// ```compile_fail
/// use tuxlink_lib::identity::IdentityHandle;
/// fn needs_serialize<T: serde::Serialize>(_t: &T) {}
/// fn _fence(h: &IdentityHandle) { needs_serialize(h); }
/// ```
#[derive(Debug, Clone)]
pub struct IdentityHandle(Arc<HandleInner>);

impl IdentityHandle {
    /// Crate-internal constructor. NOT public: only `IdentityService::authenticate`
    /// (same crate) may mint a handle, and only after keyring validation.
    pub(crate) fn new(full_callsign: Callsign) -> Self {
        IdentityHandle(Arc::new(HandleInner { full_callsign }))
    }

    /// Test-only seam (cross-phase reconciliation #2): lets Phases 3–7 build a
    /// `SessionIdentity` in unit tests without a real keyring. NOT compiled into
    /// release builds.
    #[cfg(test)]
    pub fn for_test(full_callsign: Callsign) -> Self {
        Self::new(full_callsign)
    }

    /// The authenticated licensed callsign — the Part 97 station principal.
    pub fn full_callsign(&self) -> &Callsign {
        &self.0.full_callsign
    }
}

/// The identity an operation runs as: an authenticated handle plus the address it
/// presents (`address_as`).
#[derive(Debug, Clone)]
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
        IdentityHandle::for_test(Callsign::parse(call).unwrap())
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

    #[test]
    fn handle_clone_shares_the_same_callsign() {
        // Clone is Arc-backed: a cloned handle observes the same authenticated
        // callsign (Phase 6 listeners co-hold a handle with the active session).
        let h = handle("W1ABC");
        let h2 = h.clone();
        assert_eq!(h.full_callsign(), h2.full_callsign());
    }

    #[test]
    fn session_clone_preserves_mycall_and_address() {
        let s = SessionIdentity::tactical(handle("W1ABC"), "EOC-1".into()).unwrap();
        let s2 = s.clone();
        assert_eq!(s.mycall(), s2.mycall());
        assert_eq!(s.address_as(), s2.address_as());
    }
}
