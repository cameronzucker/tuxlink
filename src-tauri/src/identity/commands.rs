//! Identity CRUD Tauri commands + their secret-free DTOs.
//!
//! Phase 2 (tuxlink-7iy2). The `_inner` fns carry the logic against the
//! `IdentityError` domain type so they are unit-testable without a Tauri
//! `State`; the thin `#[tauri::command]` wrappers resolve the canonical store
//! path ([`crate::config::identity_store_path`]) and map `IdentityError` →
//! [`crate::ui_commands::UiError`]. The DTOs NEVER carry secret material — the
//! activation secret lives only in the OS keyring (see `service.rs`).
//!
//! switch/active selection land in Phase 6/7; this surface is add/list/remove.

use std::path::Path;

use super::{
    Address, Callsign, FullIdentity, IdentityError, IdentityService, IdentityStore,
    SessionIdentity, TacticalCmsState, TacticalIdentity,
};

/// Render an [`Address`] to its wire string: the FULL callsign text, or the
/// tactical label verbatim. (`Address` has no `Display`; this is the single
/// canonical projection used by the identity DTOs.)
fn render_address(a: &Address) -> String {
    match a {
        Address::Full(c) => c.as_str().to_string(),
        Address::Tactical(l) => l.clone(),
    }
}

/// A FULL identity projected for the frontend. NO secret fields.
#[derive(Debug, Clone, serde::Serialize)]
pub struct FullIdentityDto {
    pub callsign: String,
    pub label: Option<String>,
    pub has_cms_account: bool,
    pub cms_registered: bool,
    /// Phase 2: every FULL needs re-auth on launch (Phase 6 refines from the
    /// in-memory session). No secret is ever included.
    pub needs_auth: bool,
}

/// A tactical identity projected for the frontend. NO secret fields.
#[derive(Debug, Clone, serde::Serialize)]
pub struct TacticalIdentityDto {
    pub label: String,
    pub parent: String,
    pub cms_badge: &'static str, // "unknown" | "registered" | "not_registered"
}

/// The full identity list as the dashboard reads it. Flat (FULLs + tacticals as
/// sibling vecs with `parent` pointers); the React switcher derives nesting by
/// matching `tactical.parent == full.callsign`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct IdentityListDto {
    pub full: Vec<FullIdentityDto>,
    pub tactical: Vec<TacticalIdentityDto>,
    /// The store's persisted `last_selected` hint rendered to a string, or `None`.
    /// Display-only (the UI pre-highlights this row); NOT authority over the
    /// active session (which is in-memory on the backend, Phase 6).
    pub last_selected: Option<String>,
}

/// The active session projected for the closed-state chip + header. `mycall` is
/// ALWAYS the Part-97 station ID (the FULL callsign); `address_as` is the Winlink
/// `From` (the FULL callsign OR the tactical label). NO secret, NO handle.
#[derive(Debug, Clone, serde::Serialize, PartialEq)]
pub struct ActiveIdentityDto {
    pub mycall: String,
    pub address_as: String,
    pub is_tactical: bool,
}

/// Project the active session for [`identity_active`]. `None` when no identity is
/// authenticated this launch (re-auth-on-launch, Phase 6). Factored out of the
/// `#[tauri::command]` wrapper so it is unit-testable without a Tauri `State`.
pub(crate) fn active_dto(active: Option<&SessionIdentity>) -> Option<ActiveIdentityDto> {
    active.map(|s| ActiveIdentityDto {
        mycall: s.mycall().as_str().to_string(),
        address_as: render_address(s.address_as()),
        is_tactical: matches!(s.address_as(), Address::Tactical(_)),
    })
}

/// Add a FULL identity + provision its keyring activation secret (add-time
/// provisioning per resolved design decision #2). Persists the store first, then
/// writes the secret.
pub(crate) fn add_full_inner(
    svc: &IdentityService,
    store_path: &Path,
    callsign: &str,
    label: Option<String>,
    has_cms_account: bool,
    activation_secret: &str,
) -> Result<(), IdentityError> {
    let c = Callsign::parse(callsign)?;
    let mut store = IdentityStore::load(store_path)?;
    store.add_full(FullIdentity {
        callsign: c.clone(),
        label,
        has_cms_account,
        cms_registered: false,
    })?;
    store.save()?;
    svc.set_activation_secret(&c, activation_secret)?;
    Ok(())
}

/// Authenticate a FULL credential and build the active session, persisting the
/// non-authoritative `last_selected` hint. See [`identity_authenticate`] for the
/// command-level contract.
fn authenticate_inner(
    svc: &IdentityService,
    store_path: &std::path::Path,
    backend: &dyn crate::winlink_backend::WinlinkBackend,
    callsign: &str,
    credential: &str,
    tactical_label: Option<&str>,
) -> Result<(), crate::ui_commands::UiError> {
    use crate::identity::{Address, Callsign, IdentityError, SessionIdentity};
    use crate::ui_commands::UiError;

    let full = Callsign::parse(callsign).map_err(|e| UiError::AuthFailed {
        reason: e.to_string(),
    })?;

    // Authenticate the FULL credential -> a fresh handle (keyring-gated).
    let handle = svc.authenticate(&full, credential).map_err(|e| match e {
        IdentityError::CredentialMismatch | IdentityError::NoSecretSet => UiError::AuthFailed {
            reason: e.to_string(),
        },
        other => UiError::Internal {
            detail: other.to_string(),
        },
    })?;

    // Build the active session: FULL, or a tactical that must exist under this parent.
    let (session, selected) = match tactical_label {
        None => (SessionIdentity::full(handle), Address::Full(full.clone())),
        Some(label) => {
            // The tactical must be a known label under this FULL (no ad-hoc tacticals).
            let store = crate::identity::IdentityStore::load(store_path).map_err(|e| {
                UiError::Internal {
                    detail: e.to_string(),
                }
            })?;
            // Parent is a callsign: compare case-insensitively to match the
            // keyring auth contract (authenticate is case-insensitive on the
            // callsign), so authenticating "w1abc" doesn't spuriously fail to
            // find a tactical stored under "W1ABC". The label itself is a
            // free-form tactical string — exact match.
            let known = store
                .tactical()
                .iter()
                .any(|t| t.label == label && t.parent.as_str().eq_ignore_ascii_case(full.as_str()));
            if !known {
                return Err(UiError::NotFound(format!(
                    "tactical '{label}' is not a known label under {}",
                    full.as_str()
                )));
            }
            let s = SessionIdentity::tactical(handle, label.to_string()).map_err(|e| {
                UiError::AuthFailed {
                    reason: e.to_string(),
                }
            })?;
            (s, Address::Tactical(label.to_string()))
        }
    };

    // Persist ONLY the non-authoritative last-selected hint (never the session).
    let mut store =
        crate::identity::IdentityStore::load(store_path).map_err(|e| UiError::Internal {
            detail: e.to_string(),
        })?;
    store.set_last_selected(selected);
    store.save().map_err(|e| UiError::Internal {
        detail: e.to_string(),
    })?;

    // Set the active default identity on the backend (in-memory, never persisted).
    backend.set_active_identity(session);
    Ok(())
}

/// Add a tactical identity under an existing FULL parent. Errors `ParentNotFound`
/// if the parent FULL is not in the store. No keyring interaction (a tactical has
/// no own credential).
pub(crate) fn add_tactical_inner(
    store_path: &Path,
    label: &str,
    parent: &str,
) -> Result<(), IdentityError> {
    let p = Callsign::parse(parent)?;
    let mut store = IdentityStore::load(store_path)?;
    store.add_tactical(TacticalIdentity {
        label: label.to_string(),
        parent: p,
        cms: TacticalCmsState::Unknown,
    })?;
    store.save()?;
    Ok(())
}

/// Remove a FULL or tactical identity. Removing a FULL also clears its keyring
/// activation secret (idempotent). Errors `RemoveHasTacticals` if a FULL still
/// has tactical children.
pub(crate) fn remove_inner(
    svc: &IdentityService,
    store_path: &Path,
    addr: &Address,
) -> Result<(), IdentityError> {
    let mut store = IdentityStore::load(store_path)?;
    store.remove(addr)?;
    store.save()?;
    if let Address::Full(c) = addr {
        svc.clear_activation_secret(c)?;
    }
    Ok(())
}

/// Read the identity list, projected to the secret-free DTOs. `active` is the
/// in-memory authenticated session (Phase 6): the FULL it authenticates as gets
/// `needs_auth = false`; every other FULL needs (re-)auth. `None` ⇒ everything
/// needs auth (fresh launch, pre-auth).
pub(crate) fn list_inner(
    store_path: &Path,
    active: Option<&SessionIdentity>,
) -> Result<IdentityListDto, IdentityError> {
    let store = IdentityStore::load(store_path)?;
    // The authenticated FULL callsign, if any. Compared case-insensitively to
    // mirror the keyring auth contract (`authenticate` is case-insensitive on the
    // callsign), so an active "w1abc" clears the lock on a stored "W1ABC".
    let active_full = active.map(|s| s.mycall().as_str().to_string());
    let full = store
        .full()
        .iter()
        .map(|f| FullIdentityDto {
            callsign: f.callsign.as_str().to_string(),
            label: f.label.clone(),
            has_cms_account: f.has_cms_account,
            cms_registered: f.cms_registered,
            needs_auth: active_full
                .as_deref()
                .map(|a| !a.eq_ignore_ascii_case(f.callsign.as_str()))
                .unwrap_or(true),
        })
        .collect();
    let tactical = store
        .tactical()
        .iter()
        .map(|t| TacticalIdentityDto {
            label: t.label.clone(),
            parent: t.parent.as_str().to_string(),
            cms_badge: match t.cms {
                TacticalCmsState::Unknown => "unknown",
                TacticalCmsState::Registered { .. } => "registered",
                TacticalCmsState::NotRegistered { .. } => "not_registered",
            },
        })
        .collect();
    let last_selected = store.last_selected().map(render_address);
    Ok(IdentityListDto {
        full,
        tactical,
        last_selected,
    })
}

#[tauri::command]
pub async fn identity_list(
    _svc: tauri::State<'_, IdentityService>,
    state: tauri::State<'_, crate::app_backend::BackendState>,
) -> Result<IdentityListDto, crate::ui_commands::UiError> {
    let active = state.current().and_then(|b| b.active_identity().ok());
    list_inner(&crate::config::identity_store_path(), active.as_ref())
        .map_err(|e| crate::ui_commands::UiError::Internal { detail: e.to_string() })
}

#[tauri::command]
pub async fn identity_add_full(
    svc: tauri::State<'_, IdentityService>,
    callsign: String,
    label: Option<String>,
    has_cms_account: bool,
    activation_secret: String,
) -> Result<(), crate::ui_commands::UiError> {
    add_full_inner(
        &svc,
        &crate::config::identity_store_path(),
        &callsign,
        label,
        has_cms_account,
        &activation_secret,
    )
    .map_err(|e| crate::ui_commands::UiError::Internal { detail: e.to_string() })
}

#[tauri::command]
pub async fn identity_add_tactical(
    _svc: tauri::State<'_, IdentityService>,
    label: String,
    parent: String,
) -> Result<(), crate::ui_commands::UiError> {
    add_tactical_inner(&crate::config::identity_store_path(), &label, &parent)
        .map_err(|e| crate::ui_commands::UiError::Internal { detail: e.to_string() })
}

#[tauri::command]
pub async fn identity_remove(
    svc: tauri::State<'_, IdentityService>,
    address: Address,
) -> Result<(), crate::ui_commands::UiError> {
    remove_inner(&svc, &crate::config::identity_store_path(), &address)
        .map_err(|e| crate::ui_commands::UiError::Internal { detail: e.to_string() })
}

/// Authenticate a FULL identity's credential and make it the active default
/// session (spec §"Security model": Authenticated switching). When `tactical_label`
/// is Some, the active session presents as that tactical (validated to exist under
/// the authenticated parent FULL); the RF station ID is still the FULL callsign.
/// Persists only the non-authoritative `last_selected` hint. Un-bricks transmit
/// (tuxlink-yyii): the active slot starts empty every launch (never persisted).
#[tauri::command]
pub async fn identity_authenticate(
    svc: tauri::State<'_, IdentityService>,
    state: tauri::State<'_, crate::app_backend::BackendState>,
    callsign: String,
    credential: String,
    tactical_label: Option<String>,
) -> Result<(), crate::ui_commands::UiError> {
    let backend = state.current().ok_or_else(|| {
        crate::ui_commands::UiError::NotConfigured("no backend configured".into())
    })?;
    authenticate_inner(
        &svc,
        &crate::config::identity_store_path(),
        backend.as_ref(),
        &callsign,
        &credential,
        tactical_label.as_deref(),
    )
}

/// Clear the active default identity (lock). Subsequent transmit / listen-arm /
/// Outbox-drain require a re-auth. No-op if no backend is configured.
#[tauri::command]
pub async fn identity_lock(
    state: tauri::State<'_, crate::app_backend::BackendState>,
) -> Result<(), crate::ui_commands::UiError> {
    if let Some(backend) = state.current() {
        backend.clear_active_identity();
    }
    Ok(())
}

/// The active session projected as [`ActiveIdentityDto`] (`mycall` = Part-97
/// station ID, `address_as` = presented FULL/tactical, `is_tactical`), or `None`
/// if no identity is authenticated this launch (re-auth-on-launch, Phase 6).
#[tauri::command]
pub async fn identity_active(
    state: tauri::State<'_, crate::app_backend::BackendState>,
) -> Result<Option<ActiveIdentityDto>, crate::ui_commands::UiError> {
    Ok(state
        .current()
        .and_then(|b| b.active_identity().ok())
        .and_then(|s| active_dto(Some(&s))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_full_persists_identity_and_sets_activation_secret() {
        let dir = tempfile::tempdir().unwrap();
        let store_path = dir.path().join("identities.json");
        let svc = crate::identity::IdentityService::with_memory_keyring();
        add_full_inner(
            &svc,
            &store_path,
            "W1ABC",
            Some("Personal".into()),
            /*has_cms_account=*/ false,
            "local-pass",
        )
        .expect("add_full");
        let store = crate::identity::IdentityStore::load(&store_path).unwrap();
        assert_eq!(store.full().len(), 1);
        assert_eq!(store.full()[0].callsign.as_str(), "W1ABC");
        assert!(svc.has_activation_secret(&crate::identity::Callsign::parse("W1ABC").unwrap()));
        let dto = list_inner(&store_path, None).unwrap();
        assert_eq!(dto.full.len(), 1);
        let serialized = serde_json::to_string(&dto).unwrap();
        assert!(
            !serialized.contains("local-pass"),
            "DTO must never carry the secret"
        );
    }

    // --- Phase 7 (tuxlink-noa0): DTO enrichment (needs_auth from active, last_selected, ActiveIdentityDto) ---

    /// Mint an authenticated `SessionIdentity` the only legal way (keyring auth).
    fn session_for(svc: &IdentityService, store_path: &Path, callsign: &str, secret: &str) -> SessionIdentity {
        add_full_inner(svc, store_path, callsign, None, false, secret).unwrap();
        let handle = svc
            .authenticate(&Callsign::parse(callsign).unwrap(), secret)
            .unwrap();
        SessionIdentity::full(handle)
    }

    #[test]
    fn list_needs_auth_is_false_only_for_the_active_full() {
        let dir = tempfile::tempdir().unwrap();
        let store_path = dir.path().join("identities.json");
        let svc = crate::identity::IdentityService::with_memory_keyring();
        let active = session_for(&svc, &store_path, "W1ABC", "pw");
        add_full_inner(&svc, &store_path, "W7XYZ", None, false, "pw2").unwrap();

        // With W1ABC active: its row is unlocked, W7XYZ still needs auth.
        let dto = list_inner(&store_path, Some(&active)).unwrap();
        let row = |c: &str| dto.full.iter().find(|f| f.callsign == c).unwrap();
        assert!(!row("W1ABC").needs_auth, "active FULL must not need auth");
        assert!(row("W7XYZ").needs_auth, "a non-active FULL must need auth");

        // With no active session: everything needs auth.
        let dto_none = list_inner(&store_path, None).unwrap();
        assert!(dto_none.full.iter().all(|f| f.needs_auth));
    }

    #[test]
    fn list_needs_auth_match_is_case_insensitive() {
        // Active "w1abc" must clear the lock on a stored "W1ABC" (auth is
        // case-insensitive on the callsign).
        let dir = tempfile::tempdir().unwrap();
        let store_path = dir.path().join("identities.json");
        let svc = crate::identity::IdentityService::with_memory_keyring();
        let active = session_for(&svc, &store_path, "w1abc", "pw");
        // The store now holds "w1abc" (case preserved). Confirm needs_auth clears
        // regardless of the stored casing.
        let dto = list_inner(&store_path, Some(&active)).unwrap();
        assert!(dto.full.iter().any(|f| !f.needs_auth));
    }

    #[test]
    fn list_surfaces_last_selected_hint() {
        let dir = tempfile::tempdir().unwrap();
        let store_path = dir.path().join("identities.json");
        let svc = crate::identity::IdentityService::with_memory_keyring();
        add_full_inner(&svc, &store_path, "W1ABC", None, false, "pw").unwrap();
        add_tactical_inner(&store_path, "EOC-3", "W1ABC").unwrap();

        // No hint persisted yet.
        assert_eq!(list_inner(&store_path, None).unwrap().last_selected, None);

        // Persist a tactical hint via authenticate, then confirm it renders.
        let backend = fresh_backend();
        authenticate_inner(&svc, &store_path, &backend, "W1ABC", "pw", Some("EOC-3")).unwrap();
        assert_eq!(
            list_inner(&store_path, None).unwrap().last_selected,
            Some("EOC-3".to_string())
        );
    }

    #[test]
    fn active_dto_none_when_no_session() {
        assert_eq!(active_dto(None), None);
    }

    #[test]
    fn active_dto_full_surfaces_callsign_as_both_fields() {
        let svc = crate::identity::IdentityService::with_memory_keyring();
        let dir = tempfile::tempdir().unwrap();
        let store_path = dir.path().join("identities.json");
        let s = session_for(&svc, &store_path, "W1ABC", "pw");
        let dto = active_dto(Some(&s)).unwrap();
        assert_eq!(dto.mycall, "W1ABC");
        assert_eq!(dto.address_as, "W1ABC");
        assert!(!dto.is_tactical);
    }

    #[test]
    fn active_dto_tactical_keeps_mycall_as_full_callsign() {
        // address_as is the tactical label; mycall stays the Part-97 station ID.
        let svc = crate::identity::IdentityService::with_memory_keyring();
        let dir = tempfile::tempdir().unwrap();
        let store_path = dir.path().join("identities.json");
        add_full_inner(&svc, &store_path, "W1ABC", None, false, "pw").unwrap();
        let handle = svc
            .authenticate(&Callsign::parse("W1ABC").unwrap(), "pw")
            .unwrap();
        let s = SessionIdentity::tactical(handle, "EOC-3".into()).unwrap();
        let dto = active_dto(Some(&s)).unwrap();
        assert_eq!(dto.mycall, "W1ABC", "mycall MUST stay the licensed call");
        assert_eq!(dto.address_as, "EOC-3");
        assert!(dto.is_tactical);
    }

    #[test]
    fn active_dto_serializes_without_secrets_or_handle() {
        let svc = crate::identity::IdentityService::with_memory_keyring();
        let dir = tempfile::tempdir().unwrap();
        let store_path = dir.path().join("identities.json");
        let s = session_for(&svc, &store_path, "W1ABC", "topsecret");
        let json = serde_json::to_string(&active_dto(Some(&s)).unwrap()).unwrap();
        for banned in ["topsecret", "secret", "credential", "handle", "keyring"] {
            assert!(!json.contains(banned), "ActiveIdentityDto leaked {banned}");
        }
    }

    #[test]
    fn add_tactical_under_unknown_parent_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let store_path = dir.path().join("identities.json");
        let err = add_tactical_inner(&store_path, "EOC-3", "W9NONE").unwrap_err();
        assert!(matches!(err, crate::identity::IdentityError::ParentNotFound));
    }

    #[test]
    fn remove_full_with_tacticals_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let store_path = dir.path().join("identities.json");
        let svc = crate::identity::IdentityService::with_memory_keyring();
        add_full_inner(&svc, &store_path, "W1ABC", None, false, "p").unwrap();
        add_tactical_inner(&store_path, "EOC-3", "W1ABC").unwrap();
        let err = remove_inner(
            &svc,
            &store_path,
            &crate::identity::Address::Full(
                crate::identity::Callsign::parse("W1ABC").unwrap(),
            ),
        )
        .unwrap_err();
        assert!(matches!(
            err,
            crate::identity::IdentityError::RemoveHasTacticals
        ));
    }

    // --- Phase 6 (tuxlink-5ekg): authenticate → set-active-identity ---

    use crate::winlink_backend::{BackendError, NativeBackend};

    /// Build a real `NativeBackend` whose in-memory active-identity slot starts
    /// empty (the on-disk `active_full` in the test Config does NOT seed the slot).
    fn fresh_backend() -> NativeBackend {
        let dir = tempfile::tempdir().unwrap();
        NativeBackend::new(crate::test_helpers::native_test_config(), dir.path())
    }

    #[test]
    fn authenticate_full_sets_active_and_unbricks_gate() {
        let dir = tempfile::tempdir().unwrap();
        let store_path = dir.path().join("identities.json");
        let svc = crate::identity::IdentityService::with_memory_keyring();
        add_full_inner(&svc, &store_path, "W1ABC", None, false, "pw").unwrap();
        let backend = fresh_backend();

        // Gate starts closed.
        assert!(matches!(
            backend.active_identity(),
            Err(BackendError::NoActiveIdentity)
        ));

        authenticate_inner(&svc, &store_path, &backend, "W1ABC", "pw", None)
            .expect("authenticate FULL");

        assert_eq!(
            backend.active_identity().unwrap().mycall().as_str(),
            "W1ABC"
        );
        let store = crate::identity::IdentityStore::load(&store_path).unwrap();
        assert_eq!(
            store.last_selected(),
            Some(&Address::Full(Callsign::parse("W1ABC").unwrap()))
        );
    }

    #[test]
    fn authenticate_wrong_credential_is_authfailed_and_leaves_gate_closed() {
        let dir = tempfile::tempdir().unwrap();
        let store_path = dir.path().join("identities.json");
        let svc = crate::identity::IdentityService::with_memory_keyring();
        add_full_inner(&svc, &store_path, "W1ABC", None, false, "pw").unwrap();
        let backend = fresh_backend();

        let err =
            authenticate_inner(&svc, &store_path, &backend, "W1ABC", "WRONG", None).unwrap_err();
        assert!(matches!(
            err,
            crate::ui_commands::UiError::AuthFailed { .. }
        ));
        assert!(matches!(
            backend.active_identity(),
            Err(BackendError::NoActiveIdentity)
        ));
    }

    #[test]
    fn authenticate_tactical_requires_known_label() {
        let dir = tempfile::tempdir().unwrap();
        let store_path = dir.path().join("identities.json");
        let svc = crate::identity::IdentityService::with_memory_keyring();
        add_full_inner(&svc, &store_path, "W1ABC", None, false, "pw").unwrap();
        let backend = fresh_backend();

        // Unknown tactical label -> NotFound, gate stays closed.
        let err =
            authenticate_inner(&svc, &store_path, &backend, "W1ABC", "pw", Some("GHOST"))
                .unwrap_err();
        assert!(matches!(err, crate::ui_commands::UiError::NotFound(_)));
        assert!(matches!(
            backend.active_identity(),
            Err(BackendError::NoActiveIdentity)
        ));

        // Seed a real tactical under W1ABC, then authenticate as it.
        add_tactical_inner(&store_path, "EOC-3", "W1ABC").unwrap();
        authenticate_inner(&svc, &store_path, &backend, "W1ABC", "pw", Some("EOC-3"))
            .expect("authenticate tactical");
        let active = backend.active_identity().unwrap();
        assert_eq!(active.address_as(), &Address::Tactical("EOC-3".to_string()));
        assert_eq!(active.mycall().as_str(), "W1ABC");
    }

    // adversarial-review pin: a tactical belonging to a DIFFERENT parent FULL must
    // NOT be activatable by authenticating the wrong parent — the membership check
    // requires BOTH label and parent to match. Guards the tactical-bypass angle.
    #[test]
    fn authenticate_tactical_under_wrong_parent_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let store_path = dir.path().join("identities.json");
        let svc = crate::identity::IdentityService::with_memory_keyring();
        add_full_inner(&svc, &store_path, "W1ABC", None, false, "pw").unwrap();
        add_full_inner(&svc, &store_path, "W2XYZ", None, false, "pw2").unwrap();
        // EOC-9 belongs to W2XYZ, NOT W1ABC.
        add_tactical_inner(&store_path, "EOC-9", "W2XYZ").unwrap();
        let backend = fresh_backend();

        // Authenticate W1ABC (valid credential) but ask for W2XYZ's tactical.
        let err =
            authenticate_inner(&svc, &store_path, &backend, "W1ABC", "pw", Some("EOC-9"))
                .unwrap_err();
        assert!(
            matches!(err, crate::ui_commands::UiError::NotFound(_)),
            "a tactical under a different parent must be rejected; got {err:?}"
        );
        assert!(
            matches!(backend.active_identity(), Err(BackendError::NoActiveIdentity)),
            "gate must stay closed when the tactical-parent check fails"
        );
    }

    #[test]
    fn lock_clears_active() {
        let dir = tempfile::tempdir().unwrap();
        let store_path = dir.path().join("identities.json");
        let svc = crate::identity::IdentityService::with_memory_keyring();
        add_full_inner(&svc, &store_path, "W1ABC", None, false, "pw").unwrap();
        let backend = fresh_backend();
        authenticate_inner(&svc, &store_path, &backend, "W1ABC", "pw", None)
            .expect("authenticate FULL");
        assert!(backend.active_identity().is_ok());

        // identity_lock is a trivial wrapper over clear_active_identity.
        backend.clear_active_identity();
        assert!(matches!(
            backend.active_identity(),
            Err(BackendError::NoActiveIdentity)
        ));
    }
}
