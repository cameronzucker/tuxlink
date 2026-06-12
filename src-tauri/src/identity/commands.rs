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
    TacticalCmsState, TacticalIdentity,
};

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

/// The full identity list as the dashboard reads it.
#[derive(Debug, Clone, serde::Serialize)]
pub struct IdentityListDto {
    pub full: Vec<FullIdentityDto>,
    pub tactical: Vec<TacticalIdentityDto>,
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
            let known = store
                .tactical()
                .iter()
                .any(|t| t.label == label && t.parent.as_str() == full.as_str());
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

/// Read the identity list, projected to the secret-free DTOs.
pub(crate) fn list_inner(store_path: &Path) -> Result<IdentityListDto, IdentityError> {
    let store = IdentityStore::load(store_path)?;
    let full = store
        .full()
        .iter()
        .map(|f| FullIdentityDto {
            callsign: f.callsign.as_str().to_string(),
            label: f.label.clone(),
            has_cms_account: f.has_cms_account,
            cms_registered: f.cms_registered,
            needs_auth: true,
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
    Ok(IdentityListDto { full, tactical })
}

#[tauri::command]
pub async fn identity_list(
    _svc: tauri::State<'_, IdentityService>,
) -> Result<IdentityListDto, crate::ui_commands::UiError> {
    list_inner(&crate::config::identity_store_path())
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

/// The active session's presented `Address` (FULL callsign or tactical label),
/// or `None` if no identity is authenticated this launch.
#[tauri::command]
pub async fn identity_active(
    state: tauri::State<'_, crate::app_backend::BackendState>,
) -> Result<Option<Address>, crate::ui_commands::UiError> {
    Ok(state
        .current()
        .and_then(|b| b.active_identity().ok())
        .map(|s| s.address_as().clone()))
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
        let dto = list_inner(&store_path).unwrap();
        assert_eq!(dto.full.len(), 1);
        let serialized = serde_json::to_string(&dto).unwrap();
        assert!(
            !serialized.contains("local-pass"),
            "DTO must never carry the secret"
        );
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
