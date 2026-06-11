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
}
