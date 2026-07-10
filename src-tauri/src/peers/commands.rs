//! Peers Tauri command surface + the P2P integration-matrix capability bits
//! (spec §2/§3/§4: docs/superpowers/specs/2026-07-10-p2p-peer-model-design.md).
//!
//! Mirrors the `contacts/commands.rs` house pattern: every command takes a
//! managed `State<Arc<Mutex<PeersStore>>>`, the lock is scoped in a block and
//! dropped before return, `.expect("…poisoned")` is the accepted convention, and
//! every MUTATING command emits the app-level [`PEERS_CHANGED_EVENT`]
//! (`peers:changed`) AFTER a successful flush so any other webview window (the
//! Finder, the map layer, an open settings pane — Task 22's `usePeers` hook) can
//! invalidate its cache. The event name is a frontend contract; do not rename it
//! without updating Task 22.
//!
//! Keyring cascade [R2-S7]: `peer_delete` clears the id-keyed
//! `p2p-endpoint:<peer_id>:<endpoint_id>` secret for every endpoint the store
//! reports removed, so a peer delete never orphans a stored password.

use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use super::model::{Peer, PeersFile};
use super::store::{PeersError, PeersStore};

/// App-level Tauri event emitted on every peers mutation so other webview
/// windows can invalidate their cached roster. Payload is `()`. This exact
/// string is the contract Task 22's `usePeers` hook listens on.
pub const PEERS_CHANGED_EVENT: &str = "peers:changed";

/// Emit the cross-window `peers:changed` event. Best-effort: a failed emit must
/// NOT fail the mutation (the on-disk write already succeeded).
fn emit_changed(app: &tauri::AppHandle) {
    use tauri::Emitter as _;
    let _ = app.emit(PEERS_CHANGED_EVENT, ());
}

/// The P2P integration-matrix capability flags [R5-8]. One bool per matrix row.
///
/// **Two kinds of bit — read this before adding a query site.** Exactly THREE
/// bits are UI-QUERIED and drive the render-hide mechanism (spec R5-8: a false
/// bit HIDES its row so a half-wired feature is never operator-reachable):
/// `finder_peers`, `map_peers`, and `settings_editor`. Tasks 23-25 carry absence
/// tests proving a false bit hides its row.
///
/// The other FIVE — `peer_store`, `agent_find_peers`, `agent_telnet_dial`,
/// `vara_engine_split`, and `favorites_peer_link` — are INFORMATIONAL only: the
/// agent tool / store / protocol code either exists in the binary or it does not
/// (there is no half-rendered state to hide), so nothing queries them to gate
/// rendering. They are still flipped `true` as their row lands — an honest
/// progress signal, and Task 28's "all bits true" completeness check reads every
/// one. Removing any field breaks Tasks 16/17/19/20/21 (which flip an agent /
/// store / protocol bit) and Task 28 with error[E0560]; keep all eight.
///
/// Convention: each bit starts `false` and is hardcoded `true` ONLY in the task
/// that lands its row, in that task's own commit. Task 11 lands rows 1-2 (the
/// peer store + the recorder), so [`peer_store`](Self::peer_store) is `true`
/// here; every other bit stays `false` until its task flips it.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct P2pCapabilities {
    /// Rows 1-2 — the peers store + the observation recorder. INFORMATIONAL.
    pub peer_store: bool,
    /// Rows 3, 5 — the peers read command + Finder aggregation + filter.
    /// UI-QUERIED (hides the Finder's peers surface when false).
    pub finder_peers: bool,
    /// Row 6 — peer pins on the map layer. UI-QUERIED (hides the map layer).
    pub map_peers: bool,
    /// Row 8 — the peers settings editor. UI-QUERIED (hides the editor).
    pub settings_editor: bool,
    /// Row 4 — the `find_peers` agent tool. INFORMATIONAL.
    pub agent_find_peers: bool,
    /// Row 7 — the agent telnet-dial path. INFORMATIONAL.
    pub agent_telnet_dial: bool,
    /// Row 9 — the VARA engine split. INFORMATIONAL.
    pub vara_engine_split: bool,
    /// Row 10 — the favorites↔peer link. INFORMATIONAL.
    pub favorites_peer_link: bool,
}

/// Read the whole peers file (roster + schema_version).
#[tauri::command]
pub fn peers_read(svc: tauri::State<Arc<Mutex<PeersStore>>>) -> Result<PeersFile, PeersError> {
    let store = svc.lock().expect("peers store mutex poisoned");
    Ok(store.file().clone())
}

/// Insert or replace an operator-added peer by `id`. Validates the callsign
/// fields at the store's presented-callsign write boundary (a portable form like
/// `W6ABC/P` is accepted), persists, and emits `peers:changed`.
#[tauri::command]
pub fn peer_upsert(
    app: tauri::AppHandle,
    svc: tauri::State<Arc<Mutex<PeersStore>>>,
    peer: Peer,
) -> Result<(), PeersError> {
    {
        let mut store = svc.lock().expect("peers store mutex poisoned");
        store.upsert_manual(peer)?;
    }
    emit_changed(&app);
    Ok(())
}

/// Delete a peer by id (no-op if absent). Cascades the keyring-secret clear for
/// every endpoint the store reports removed [R2-S7] so no password is orphaned,
/// then emits `peers:changed`. A keyring delete failure is logged (never the
/// secret) but does not fail the command — the roster write already succeeded.
#[tauri::command]
pub fn peer_delete(
    app: tauri::AppHandle,
    svc: tauri::State<Arc<Mutex<PeersStore>>>,
    id: String,
) -> Result<(), PeersError> {
    let endpoint_ids = {
        let mut store = svc.lock().expect("peers store mutex poisoned");
        store.delete_peer(&id)?
    };
    for endpoint_id in &endpoint_ids {
        if let Err(e) =
            crate::winlink::credentials::p2p_endpoint_password_delete(&id, endpoint_id)
        {
            tracing::warn!(
                target: "tuxlink::peers",
                peer_id = %id,
                endpoint_id = %endpoint_id,
                "peer delete: clearing endpoint keyring secret failed: {e}"
            );
        }
    }
    emit_changed(&app);
    Ok(())
}

/// Merge `absorb_id` into `keep_id` (dedup presented forms / channels / endpoints
/// onto the kept record), then emit `peers:changed`.
///
/// The store returns the absorbed endpoint ids for the keyring re-key cascade,
/// but re-keying `p2p-endpoint:<absorb_id>:<eid>` → `p2p-endpoint:<keep_id>:<eid>`
/// is Task 10/20 territory (the store doc attributes it there); this command does
/// not perform it, so the ids are intentionally dropped here.
#[tauri::command]
pub fn peer_merge(
    app: tauri::AppHandle,
    svc: tauri::State<Arc<Mutex<PeersStore>>>,
    keep_id: String,
    absorb_id: String,
) -> Result<(), PeersError> {
    let _absorbed_endpoint_ids = {
        let mut store = svc.lock().expect("peers store mutex poisoned");
        store.merge(&keep_id, &absorb_id)?
    };
    emit_changed(&app);
    Ok(())
}

/// Split the named presented forms off `peer_id` into a fresh `do_not_merge`
/// record, emit `peers:changed`, and return the new record's id.
#[tauri::command]
pub fn peer_split(
    app: tauri::AppHandle,
    svc: tauri::State<Arc<Mutex<PeersStore>>>,
    peer_id: String,
    presented: Vec<String>,
) -> Result<String, PeersError> {
    let new_id = {
        let mut store = svc.lock().expect("peers store mutex poisoned");
        store.split(&peer_id, presented, chrono::Local::now().to_rfc3339())?
    };
    emit_changed(&app);
    Ok(new_id)
}

/// Promote an endpoint to `Operator` provenance IN PLACE (the only path that
/// writes `Operator`, so the id-keyed keyring secret is never orphaned [R5-5]),
/// then emit `peers:changed`.
#[tauri::command]
pub fn peer_endpoint_promote(
    app: tauri::AppHandle,
    svc: tauri::State<Arc<Mutex<PeersStore>>>,
    peer_id: String,
    endpoint_id: String,
) -> Result<(), PeersError> {
    {
        let mut store = svc.lock().expect("peers store mutex poisoned");
        store.promote_endpoint(&peer_id, &endpoint_id)?;
    }
    emit_changed(&app);
    Ok(())
}

/// Set the keyring password for a specific endpoint. Verifies the
/// `(peer_id, endpoint_id)` pair exists in the roster first so a secret is never
/// written for a non-existent endpoint (which would orphan it immediately). No
/// `peers:changed` emit — the roster is unchanged; only the keyring is touched.
#[tauri::command]
pub fn peer_endpoint_password_set(
    svc: tauri::State<Arc<Mutex<PeersStore>>>,
    peer_id: String,
    endpoint_id: String,
    password: String,
) -> Result<(), PeersError> {
    {
        let store = svc.lock().expect("peers store mutex poisoned");
        let exists = store
            .file()
            .peers
            .iter()
            .find(|p| p.id == peer_id)
            .is_some_and(|p| p.endpoints.iter().any(|e| e.id == endpoint_id));
        if !exists {
            return Err(PeersError::Validation(format!(
                "no endpoint {endpoint_id:?} on peer {peer_id:?}"
            )));
        }
    }
    crate::winlink::credentials::p2p_endpoint_password_write(&peer_id, &endpoint_id, &password)
        .map_err(PeersError::Io)?;
    Ok(())
}

/// Clear the keyring password for a specific endpoint. Idempotent (a missing
/// entry is success). No `peers:changed` emit — the roster is unchanged.
#[tauri::command]
pub fn peer_endpoint_password_clear(
    svc: tauri::State<Arc<Mutex<PeersStore>>>,
    peer_id: String,
    endpoint_id: String,
) -> Result<(), PeersError> {
    // `svc` is part of the frontend-facing signature for symmetry with `_set`;
    // clear is idempotent, so a missing endpoint is not an error and no lookup is
    // required.
    let _ = svc;
    crate::winlink::credentials::p2p_endpoint_password_delete(&peer_id, &endpoint_id)
        .map_err(PeersError::Io)?;
    Ok(())
}

/// Report the P2P integration-matrix capability bits [R5-8]. See
/// [`P2pCapabilities`] for the UI-queried-vs-informational distinction. Task 11
/// lands rows 1-2, so `peer_store` is `true`; every other bit is `false` until
/// its own task flips it.
#[tauri::command]
pub fn p2p_capabilities() -> P2pCapabilities {
    P2pCapabilities {
        peer_store: true,
        finder_peers: false,
        map_peers: false,
        settings_editor: false,
        agent_find_peers: false,
        agent_telnet_dial: false,
        vara_engine_split: false,
        favorites_peer_link: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peers_changed_event_name_is_the_frontend_contract() {
        // Task 22's usePeers hook invalidates on this exact string; a rename here
        // silently breaks cross-window invalidation.
        assert_eq!(PEERS_CHANGED_EVENT, "peers:changed");
    }

    #[test]
    fn capabilities_report_only_peer_store_true_at_task_11() {
        // Task 11 lands rows 1-2 (store + recorder). Every other bit stays false
        // until its own task flips it — this guards against an accidental early
        // flip and pins Task 28's completeness baseline.
        let c = p2p_capabilities();
        assert!(c.peer_store, "Task 11 lands the store + recorder");
        assert!(!c.finder_peers);
        assert!(!c.map_peers);
        assert!(!c.settings_editor);
        assert!(!c.agent_find_peers);
        assert!(!c.agent_telnet_dial);
        assert!(!c.vara_engine_split);
        assert!(!c.favorites_peer_link);
    }

    #[test]
    fn capabilities_serialize_camelless_snake_case_on_the_wire() {
        // The struct carries no serde(rename_all), so the field names ARE the
        // wire keys — the contract Task 23-25 + Task 28 read. Pin it.
        let v = serde_json::to_value(p2p_capabilities()).unwrap();
        assert_eq!(v.get("peer_store").and_then(|b| b.as_bool()), Some(true));
        assert_eq!(v.get("finder_peers").and_then(|b| b.as_bool()), Some(false));
        assert!(v.get("favorites_peer_link").is_some());
    }
}
