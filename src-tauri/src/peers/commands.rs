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
//! reports removed, and `peer_merge` re-keys/deletes secrets per the store's
//! [`AbsorbedEndpoint`] dispositions — so no roster mutation ever orphans a
//! stored password.

use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::favorites::store::FavoritesStore;

use super::model::{Peer, PeersFile};
use super::store::{AbsorbedEndpoint, PeersError, PeersStore};

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
/// then cascades into the favorites store [R4-12]: every `Favorite` back-linked
/// to this peer id (a starred or recent channel) is removed via
/// [`FavoritesStore::delete_by_peer_id`] so a deleted peer never leaves an
/// orphaned star. Finally emits `peers:changed`. A keyring delete failure or a
/// favorites-cascade failure is logged but does not fail the command — the
/// roster write already succeeded.
///
/// Lock order is peers→favorites here and nowhere reversed: the peers lock is
/// acquired and released first (inside the `delete_peer` block), and the
/// favorites lock is taken afterward in its own scope — the two are never held
/// simultaneously, mirroring `recorder.rs`'s documented store→limiter ordering.
#[tauri::command]
pub fn peer_delete(
    app: tauri::AppHandle,
    svc: tauri::State<Arc<Mutex<PeersStore>>>,
    favorites: tauri::State<Arc<Mutex<FavoritesStore>>>,
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
    {
        let mut store = favorites.lock().expect("favorites store mutex poisoned");
        if let Err(e) = store.delete_by_peer_id(&id) {
            tracing::warn!(
                target: "tuxlink::peers",
                peer_id = %id,
                "peer delete: clearing back-linked favorites failed: {e}"
            );
        }
    }
    emit_changed(&app);
    Ok(())
}

/// Re-key / delete keyring secrets after a merge, per the store's
/// [`AbsorbedEndpoint`] dispositions. Pure orchestration over injected
/// keyring ops so it is deterministically testable without a real keyring;
/// [`peer_merge`] supplies the production `p2p_endpoint_password_*` fns.
///
/// - `Survived { endpoint_id }`: the endpoint kept its id but moved under
///   `keep_id`, so future lookups use `(keep_id, eid)` — read the secret at
///   `(absorb_id, eid)`, write it to `(keep_id, eid)`, then delete the old
///   entry (strictly after the write succeeds, so there is no window where
///   the secret exists in neither account). A read-miss (`None`) is normal
///   (no secret was ever set) — nothing to do.
/// - `Deduped { endpoint_id }`: the endpoint was dropped in favor of an
///   existing keep endpoint, so its account has no valid target — delete it.
///
/// Best-effort, mirroring the Task 10 migration-orphan posture: a backend
/// error on any step warns (account ids only — NEVER the secret value) and
/// continues with the remaining endpoints; the roster write already
/// succeeded, so the command must not fail. A `NoEntry` delete is silent
/// (the delete API is idempotent).
fn rekey_merged_endpoint_secrets<R, W, D>(
    keep_id: &str,
    absorb_id: &str,
    dispositions: &[AbsorbedEndpoint],
    read: R,
    write: W,
    delete: D,
) where
    R: Fn(&str, &str) -> Result<Option<String>, String>,
    W: Fn(&str, &str, &str) -> Result<(), String>,
    D: Fn(&str, &str) -> Result<(), String>,
{
    for d in dispositions {
        match d {
            AbsorbedEndpoint::Survived { endpoint_id } => {
                let secret = match read(absorb_id, endpoint_id) {
                    Ok(Some(s)) => s,
                    Ok(None) => continue, // no secret was ever set — nothing to move
                    Err(e) => {
                        tracing::warn!(
                            target: "tuxlink::peers",
                            absorb_id = %absorb_id,
                            endpoint_id = %endpoint_id,
                            "merge re-key: reading absorbed endpoint secret failed: {e}"
                        );
                        continue;
                    }
                };
                if let Err(e) = write(keep_id, endpoint_id, &secret) {
                    tracing::warn!(
                        target: "tuxlink::peers",
                        keep_id = %keep_id,
                        endpoint_id = %endpoint_id,
                        "merge re-key: writing secret under the kept peer failed; \
                         the secret remains under the absorbed id: {e}"
                    );
                    continue; // do NOT delete the old entry — it still holds the secret
                }
                if let Err(e) = delete(absorb_id, endpoint_id) {
                    tracing::warn!(
                        target: "tuxlink::peers",
                        absorb_id = %absorb_id,
                        endpoint_id = %endpoint_id,
                        "merge re-key: secret moved to the kept peer, but deleting \
                         the old entry failed; the old entry is orphaned and can be \
                         removed manually: {e}"
                    );
                }
            }
            AbsorbedEndpoint::Deduped { endpoint_id } => {
                if let Err(e) = delete(absorb_id, endpoint_id) {
                    tracing::warn!(
                        target: "tuxlink::peers",
                        absorb_id = %absorb_id,
                        endpoint_id = %endpoint_id,
                        "merge: deleting the deduped endpoint's secret failed; \
                         the entry is orphaned and can be removed manually: {e}"
                    );
                }
            }
        }
    }
}

/// Merge `absorb_id` into `keep_id` (dedup presented forms / channels / endpoints
/// onto the kept record), cascade the keyring secrets per the store's
/// [`AbsorbedEndpoint`] dispositions [R2-S7] — a `Survived` endpoint's secret is
/// re-keyed `(absorb_id, eid)` → `(keep_id, eid)`, a `Deduped` endpoint's secret
/// is deleted — then emit `peers:changed`. Keyring failures warn (never the
/// secret) but do not fail the command; the roster write already succeeded.
#[tauri::command]
pub fn peer_merge(
    app: tauri::AppHandle,
    svc: tauri::State<Arc<Mutex<PeersStore>>>,
    keep_id: String,
    absorb_id: String,
) -> Result<(), PeersError> {
    let dispositions = {
        let mut store = svc.lock().expect("peers store mutex poisoned");
        store.merge(&keep_id, &absorb_id)?
    };
    rekey_merged_endpoint_secrets(
        &keep_id,
        &absorb_id,
        &dispositions,
        crate::winlink::credentials::p2p_endpoint_password_read,
        crate::winlink::credentials::p2p_endpoint_password_write,
        crate::winlink::credentials::p2p_endpoint_password_delete,
    );
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
        finder_peers: true, // Task 26 (R5-8 rows 3+5): the Finder's peers surface landed (Tasks 22-23).
        map_peers: true,    // Task 26 (R5-8 row 6): peer pins on the map layer landed (Task 24).
        settings_editor: false, // Task 25 flips this when the peers settings editor lands.
        agent_find_peers: true, // Task 19 (R5-8 row 4): the find_peers agent tool landed.
        // Task 20 landed the agent telnet-dial path, then Task T-A reverted it
        // (operator pivot: a telnet host:port is destination-trust the armed
        // egress gate cannot vouch for). Row 7 is false again pending a
        // redesign, if any.
        agent_telnet_dial: false,
        vara_engine_split: true, // Task 21 (R5-8 row 9): agent VARA egress dispatches on engine.
        favorites_peer_link: true, // Task 17 (R5-7): the favorites↔peer bridge landed.
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
    fn capabilities_report_only_landed_rows_true() {
        // Task 11 lands rows 1-2 (store + recorder); Task 17 lands row 10 (the
        // favorites↔peer bridge); Task 19 lands row 4 (the find_peers agent tool);
        // Task 21 lands row 9 (the VARA engine split); Task 26 lands rows 3+5
        // (Finder peers surface, Tasks 22-23) and row 6 (map peer pins, Task 24).
        // `settings_editor` stays false until Task 25 lands its own row — this
        // guards against an accidental early flip and pins Task 28's completeness
        // baseline. Row 7 (`agent_telnet_dial`) landed in Task 20 and was
        // reverted by Task T-A (operator pivot); it stays false.
        let c = p2p_capabilities();
        assert!(c.peer_store, "Task 11 lands the store + recorder");
        assert!(c.finder_peers, "Task 26 lands the Finder's peers surface");
        assert!(c.map_peers, "Task 26 lands the map layer's peer pins");
        assert!(!c.settings_editor, "Task 25 has not landed yet");
        assert!(c.agent_find_peers, "Task 19 lands the find_peers agent tool");
        assert!(
            !c.agent_telnet_dial,
            "Task 20's agent telnet-dial path was reverted by Task T-A"
        );
        assert!(c.vara_engine_split, "Task 21 lands the VARA engine split");
        assert!(c.favorites_peer_link, "Task 17 lands the favorites↔peer bridge");
    }

    // ------------------------------------------------------------------
    // rekey_merged_endpoint_secrets — the peer_merge keyring cascade.
    //
    // The `#[tauri::command]` wrapper adds only State/AppHandle extraction
    // (mirroring the contacts/favorites test posture); the cascade logic lives
    // in the injected-ops core fn, exercised here against a fake keyring
    // (RefCell<HashMap<(peer_id, endpoint_id), secret>>).
    // ------------------------------------------------------------------

    type FakeKeyring = std::cell::RefCell<std::collections::HashMap<(String, String), String>>;

    fn run_cascade(kr: &FakeKeyring, keep: &str, absorb: &str, d: &[AbsorbedEndpoint]) {
        rekey_merged_endpoint_secrets(
            keep,
            absorb,
            d,
            |p, e| Ok(kr.borrow().get(&(p.to_string(), e.to_string())).cloned()),
            |p, e, s| {
                kr.borrow_mut()
                    .insert((p.to_string(), e.to_string()), s.to_string());
                Ok(())
            },
            |p, e| {
                kr.borrow_mut().remove(&(p.to_string(), e.to_string()));
                Ok(()) // idempotent, like p2p_endpoint_password_delete
            },
        );
    }

    #[test]
    fn merge_cascade_rekeys_survived_and_deletes_deduped_secrets() {
        let kr: FakeKeyring = Default::default();
        kr.borrow_mut()
            .insert(("p-absorb".into(), "e-uniq".into()), "s3cret-uniq".into());
        kr.borrow_mut()
            .insert(("p-absorb".into(), "e-dup".into()), "s3cret-dup".into());

        run_cascade(
            &kr,
            "p-keep",
            "p-absorb",
            &[
                AbsorbedEndpoint::Deduped {
                    endpoint_id: "e-dup".into(),
                },
                AbsorbedEndpoint::Survived {
                    endpoint_id: "e-uniq".into(),
                },
            ],
        );

        let map = kr.borrow();
        assert_eq!(
            map.get(&("p-keep".into(), "e-uniq".into())).map(String::as_str),
            Some("s3cret-uniq"),
            "survived endpoint's secret is readable under (keep_id, eid)"
        );
        assert!(
            !map.contains_key(&("p-absorb".into(), "e-uniq".into())),
            "old survived account is gone after the re-key"
        );
        assert!(
            !map.contains_key(&("p-absorb".into(), "e-dup".into())),
            "deduped endpoint's secret is deleted (its id exists nowhere)"
        );
        assert!(
            !map.contains_key(&("p-keep".into(), "e-dup".into())),
            "a deduped secret is never re-keyed onto the kept peer"
        );
    }

    #[test]
    fn merge_cascade_survived_with_no_secret_is_a_silent_noop() {
        let kr: FakeKeyring = Default::default();
        run_cascade(
            &kr,
            "p-keep",
            "p-absorb",
            &[AbsorbedEndpoint::Survived {
                endpoint_id: "e-uniq".into(),
            }],
        );
        assert!(kr.borrow().is_empty(), "no secret existed → nothing written");
    }

    #[test]
    fn merge_cascade_write_failure_keeps_the_old_entry() {
        // Orphan posture: if the new-key write fails, the old entry must NOT
        // be deleted — it is the only copy of the secret.
        let kr: FakeKeyring = Default::default();
        kr.borrow_mut()
            .insert(("p-absorb".into(), "e-uniq".into()), "s3cret".into());
        rekey_merged_endpoint_secrets(
            "p-keep",
            "p-absorb",
            &[AbsorbedEndpoint::Survived {
                endpoint_id: "e-uniq".into(),
            }],
            |p, e| Ok(kr.borrow().get(&(p.to_string(), e.to_string())).cloned()),
            |_, _, _| Err("simulated backend write failure".to_string()),
            |p, e| {
                kr.borrow_mut().remove(&(p.to_string(), e.to_string()));
                Ok(())
            },
        );
        assert_eq!(
            kr.borrow()
                .get(&("p-absorb".into(), "e-uniq".into()))
                .map(String::as_str),
            Some("s3cret"),
            "old entry survives when the re-key write fails — the secret is never lost"
        );
    }

    #[test]
    fn capabilities_serialize_camelless_snake_case_on_the_wire() {
        // The struct carries no serde(rename_all), so the field names ARE the
        // wire keys — the contract Task 23-25 + Task 28 read. Pin it.
        let v = serde_json::to_value(p2p_capabilities()).unwrap();
        assert_eq!(v.get("peer_store").and_then(|b| b.as_bool()), Some(true));
        assert_eq!(v.get("finder_peers").and_then(|b| b.as_bool()), Some(true));
        assert_eq!(v.get("settings_editor").and_then(|b| b.as_bool()), Some(false));
        assert!(v.get("favorites_peer_link").is_some());
    }
}
