//! Contacts tauri commands + id/timestamp stamping — Task A2.
//!
//! Plan: docs/superpowers/plans/2026-06-07-contacts-favorites.md → "Locked
//! decisions" + "### Task A2". The [`store`](super::store) layer takes
//! fully-formed `Contact`/`Group` values; id + RFC3339-UTC timestamp STAMPING
//! is THIS (command) layer's job.
//!
//! **Stamping contract:**
//! - A NEW entity (empty `id`) gets a fresh uuid-v4 `id` and
//!   `created_at == updated_at == now`.
//! - An UPDATE (non-empty `id`) PRESERVES the caller-supplied `created_at`,
//!   keeps the `id`, and sets `updated_at = now`.
//!
//! The pure helpers ([`stamp_contact`]/[`stamp_group`]) take an injected `now`
//! string and a `new_id` factory so they are deterministically testable without
//! a Tauri harness; the thin `#[tauri::command]` wrappers below call them with
//! the real wall clock + a uuid-v4 factory.
//!
//! **Cross-window invalidation (H9):** every mutating command emits the
//! app-level Tauri event [`CONTACTS_CHANGED_EVENT`] (`contacts:changed`) AFTER a
//! successful flush, so a separate webview window (e.g. an open Compose) can
//! invalidate its `useContacts` cache. The frontend listener lands in Task A4.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use super::reachability::{ContactTier, Origin, Provenance};
use super::store::{Contact, ContactsError, ContactsFile, ContactsStore, Group};
use super::suggest::{derive_suggestions, Suggestion};
use crate::app_backend::BackendState;
use crate::favorites::store::{ConnectionAttempt, FavoritesStore, TodHint};
use crate::winlink_backend::{MailboxFolder, MessageMeta};

/// App-level Tauri event emitted on every contacts mutation so other webview
/// windows can invalidate their cached contacts (H9). Payload is `()`.
pub const CONTACTS_CHANGED_EVENT: &str = "contacts:changed";

/// Mint a fresh uuid-v4 string id (mirrors `search/saved.rs:93`,
/// `forms/draft_library.rs:208`).
pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// Stamp a contact for persistence. A NEW contact (empty `id`) gets a fresh id
/// (via `new_id`) and `created_at == updated_at == now`; an UPDATE preserves the
/// existing `id` + `created_at` and sets `updated_at = now`.
///
/// Pure + deterministic: `now` and `new_id` are injected so this is unit-tested
/// without a Tauri harness.
pub fn stamp_contact(mut c: Contact, now: &str, new_id: impl FnOnce() -> String) -> Contact {
    if c.id.trim().is_empty() {
        c.id = new_id();
        c.created_at = now.to_string();
    }
    // created_at is preserved on update (caller supplies the original).
    c.updated_at = now.to_string();
    c
}

/// Stamp a group for persistence. Same new-vs-update semantics as
/// [`stamp_contact`].
pub fn stamp_group(mut g: Group, now: &str, new_id: impl FnOnce() -> String) -> Group {
    if g.id.trim().is_empty() {
        g.id = new_id();
        g.created_at = now.to_string();
    }
    g.updated_at = now.to_string();
    g
}

/// The current wall-clock instant as an RFC3339 UTC string (e.g.
/// `2026-06-08T12:34:56+00:00`).
fn now_utc() -> String {
    chrono::Utc::now().to_rfc3339()
}

/// Emit the cross-window `contacts:changed` event (H9). Best-effort: a failed
/// emit must NOT fail the mutation (the on-disk write already succeeded).
fn emit_changed(app: &tauri::AppHandle) {
    use tauri::Emitter as _;
    let _ = app.emit(CONTACTS_CHANGED_EVENT, ());
}

/// Read the whole contacts file (contacts + groups + schema_version).
#[tauri::command]
pub fn contacts_read(
    svc: tauri::State<Arc<Mutex<ContactsStore>>>,
) -> Result<ContactsFile, ContactsError> {
    let store = svc.lock().expect("contacts store mutex poisoned");
    Ok(store.file().clone())
}

/// Merge an operator upsert with the stored record (tier + reachability
/// preservation, spec §AMENDMENT pt. 1). Pure + deterministic.
///
/// - NEW record (no stored match): `tier = Confirmed` and `origin = Manual` —
///   the operator-created ("added") path; auto-creation is the observation
///   recorder's job and NEVER lands confirmed, this path ALWAYS does.
/// - UPDATE: `tier` / `origin` / `grid` / `channels` / `endpoints` are taken
///   FROM THE STORED record. The editor owns identity fields only; an upsert
///   must not silently flip an existing record's tier ([`contact_confirm`] is
///   the only tier writer) nor wipe observed reachability with the editor's
///   snapshot.
pub fn merge_for_upsert(stored: Option<&Contact>, mut incoming: Contact) -> Contact {
    match stored {
        Some(s) => {
            incoming.tier = s.tier;
            incoming.origin = s.origin;
            incoming.grid = s.grid.clone();
            incoming.channels = s.channels.clone();
            incoming.endpoints = s.endpoints.clone();
        }
        None => {
            incoming.tier = ContactTier::Confirmed;
            incoming.origin = Origin::Manual;
        }
    }
    incoming
}

/// Insert or update a contact. Stamps id (if empty) + timestamps, merges tier
/// + reachability per [`merge_for_upsert`] (a new record is Confirmed/added;
/// an update never flips tier or wipes observed reachability), persists,
/// emits `contacts:changed`, and returns the STORED contact (so the caller
/// learns the assigned id + timestamps).
#[tauri::command]
pub fn contact_upsert(
    contact: Contact,
    svc: tauri::State<Arc<Mutex<ContactsStore>>>,
    app: tauri::AppHandle,
) -> Result<Contact, ContactsError> {
    let stamped = stamp_contact(contact, &now_utc(), new_id);
    let merged = {
        let mut store = svc.lock().expect("contacts store mutex poisoned");
        let stored = store
            .file()
            .contacts
            .iter()
            .find(|c| c.id == stamped.id)
            .cloned();
        let merged = merge_for_upsert(stored.as_ref(), stamped);
        store.contact_upsert(merged.clone())?;
        merged
    };
    emit_changed(&app);
    Ok(merged)
}

/// Delete a contact by id (no-op if absent). Cascades the id-keyed endpoint
/// keyring secrets [R2-S7] for every endpoint the store reports removed (no
/// roster mutation ever orphans a stored password), then cascades into the
/// favorites store [R4-12]: every favorite back-linked to this contact id is
/// removed so a deleted contact never leaves an orphaned star. Persists +
/// emits `contacts:changed`. A keyring or favorites-cascade failure is logged
/// but does not fail the command — the roster write already succeeded.
///
/// Lock order is contacts→favorites and nowhere reversed: the contacts lock
/// is released before the favorites lock is taken.
#[tauri::command]
pub fn contact_delete(
    id: String,
    svc: tauri::State<Arc<Mutex<ContactsStore>>>,
    favorites: tauri::State<Arc<Mutex<FavoritesStore>>>,
    app: tauri::AppHandle,
) -> Result<(), ContactsError> {
    let endpoint_ids = {
        let mut store = svc.lock().expect("contacts store mutex poisoned");
        store.contact_delete(&id)?
    };
    for endpoint_id in &endpoint_ids {
        if let Err(e) = crate::winlink::credentials::p2p_endpoint_password_delete(&id, endpoint_id)
        {
            tracing::warn!(
                target: "tuxlink::contacts",
                contact_id = %id,
                endpoint_id = %endpoint_id,
                "contact delete: clearing endpoint keyring secret failed: {e}"
            );
        }
    }
    {
        let mut store = favorites.lock().expect("favorites store mutex poisoned");
        if let Err(e) = store.delete_by_contact_id(&id) {
            tracing::warn!(
                target: "tuxlink::contacts",
                contact_id = %id,
                "contact delete: clearing back-linked favorites failed: {e:?}"
            );
        }
    }
    emit_changed(&app);
    Ok(())
}

/// Flip a contact `Unconfirmed → Confirmed` — the one-click promote (spec
/// §AMENDMENT pt. 7). Idempotent on an already-confirmed record; errors on an
/// unknown id. Persists + emits `contacts:changed`.
#[tauri::command]
pub fn contact_confirm(
    id: String,
    svc: tauri::State<Arc<Mutex<ContactsStore>>>,
    app: tauri::AppHandle,
) -> Result<(), ContactsError> {
    {
        let mut store = svc.lock().expect("contacts store mutex poisoned");
        store.contact_confirm(&id, now_utc())?;
    }
    emit_changed(&app);
    Ok(())
}

/// Validate that a password may be attached to `(contact_id, endpoint_id)`:
/// the pair must exist AND the endpoint must be `Provenance::Operator` — a
/// stored password attaches only to an operator-entered endpoint and is never
/// usable with an observed one (spec [R2-S7], §AMENDMENT pt. 3). Pure over
/// the file so the trust boundary is unit-testable without a Tauri harness.
pub fn endpoint_password_set_guard(
    file: &ContactsFile,
    contact_id: &str,
    endpoint_id: &str,
) -> Result<(), ContactsError> {
    let ep = file
        .contacts
        .iter()
        .find(|c| c.id == contact_id)
        .and_then(|c| c.endpoints.iter().find(|e| e.id == endpoint_id));
    match ep {
        None => Err(ContactsError::Validation(format!(
            "no endpoint {endpoint_id:?} on contact {contact_id:?}"
        ))),
        Some(e) if e.provenance == Provenance::Operator => Ok(()),
        Some(_) => Err(ContactsError::Validation(
            "passwords attach only to operator-entered endpoints".to_string(),
        )),
    }
}

/// Set the keyring password for a specific contact endpoint. Enforces the
/// [R2-S7] boundary via [`endpoint_password_set_guard`] (pair must exist and
/// be operator-provenance) so a secret is never written for a non-existent or
/// observed endpoint. No `contacts:changed` emit — the roster is unchanged;
/// only the keyring is touched.
#[tauri::command]
pub fn contact_endpoint_password_set(
    svc: tauri::State<Arc<Mutex<ContactsStore>>>,
    contact_id: String,
    endpoint_id: String,
    password: String,
) -> Result<(), ContactsError> {
    {
        let store = svc.lock().expect("contacts store mutex poisoned");
        endpoint_password_set_guard(store.file(), &contact_id, &endpoint_id)?;
    }
    crate::winlink::credentials::p2p_endpoint_password_write(&contact_id, &endpoint_id, &password)
        .map_err(ContactsError::Io)?;
    Ok(())
}

/// Clear the keyring password for a specific contact endpoint. Idempotent (a
/// missing entry is success; no existence lookup required). No
/// `contacts:changed` emit — the roster is unchanged.
#[tauri::command]
pub fn contact_endpoint_password_clear(
    contact_id: String,
    endpoint_id: String,
) -> Result<(), ContactsError> {
    crate::winlink::credentials::p2p_endpoint_password_delete(&contact_id, &endpoint_id)
        .map_err(ContactsError::Io)?;
    Ok(())
}

/// Insert or update a group. Stamps id (if empty) + timestamps, persists, emits
/// `contacts:changed`, returns the STORED group.
#[tauri::command]
pub fn group_upsert(
    group: Group,
    svc: tauri::State<Arc<Mutex<ContactsStore>>>,
    app: tauri::AppHandle,
) -> Result<Group, ContactsError> {
    let stamped = stamp_group(group, &now_utc(), new_id);
    {
        let mut store = svc.lock().expect("contacts store mutex poisoned");
        store.group_upsert(stamped.clone())?;
    }
    emit_changed(&app);
    Ok(stamped)
}

/// Delete a group by id (no-op if absent). Persists + emits `contacts:changed`.
#[tauri::command]
pub fn group_delete(
    id: String,
    svc: tauri::State<Arc<Mutex<ContactsStore>>>,
    app: tauri::AppHandle,
) -> Result<(), ContactsError> {
    {
        let mut store = svc.lock().expect("contacts store mutex poisoned");
        store.group_delete(&id)?;
    }
    emit_changed(&app);
    Ok(())
}

/// Read the operator's own callsign from config for self-exclusion (H11).
///
/// Prefers `identity.callsign` (the CMS path); falls back to
/// `identity.identifier` (the offline-mode station identifier) since either may
/// appear as the `From` on Sent/Outbox. Returns an empty string when no config
/// exists yet (pre-wizard) — `derive_suggestions` treats a blank operator
/// callsign as "no self-exclusion key", which is correct.
fn operator_callsign() -> String {
    match crate::config::read_config() {
        Ok(cfg) => cfg
            .identity
            .active_full
            .or(cfg.identity.identifier)
            .unwrap_or_default(),
        Err(_) => String::new(),
    }
}

/// Tally per-correspondent message counts across the supplied message metas.
///
/// Counts BOTH the `From` and every `To` recipient of each message, so a
/// correspondent who only ever appears as a recipient is still surfaced.
/// Correspondents are keyed CASE-INSENSITIVELY across messages (so `KE7VAR`
/// and `ke7var` in different messages tally to one entry) while preserving the
/// first-seen display form for the card label. Counts are de-duplicated PER
/// MESSAGE (a callsign listed twice in one message counts once for that
/// message). Returns `(correspondent, count)` pairs in arbitrary order —
/// `derive_suggestions` imposes the final sort.
fn tally_correspondents(metas: &[MessageMeta]) -> Vec<(String, u32)> {
    // key (uppercased) → (first-seen display form, count)
    let mut counts: HashMap<String, (String, u32)> = HashMap::new();
    for m in metas {
        let mut seen_this_msg: std::collections::HashSet<String> =
            std::collections::HashSet::new();
        let mut bump = |addr: &str| {
            let a = addr.trim();
            if a.is_empty() {
                return;
            }
            let key = a.to_ascii_uppercase();
            // De-dup within a single message so one message contributes at most
            // 1 to any correspondent's count.
            if seen_this_msg.insert(key.clone()) {
                let entry = counts.entry(key).or_insert_with(|| (a.to_string(), 0));
                entry.1 += 1;
            }
        };
        bump(&m.from);
        for to in &m.to {
            bump(to);
        }
    }
    counts.into_values().collect()
}

/// Derive suggest-from-history "+ Add" cards from the mailbox (Task A3).
///
/// Enumerates correspondents across the system folders (Inbox/Sent/Outbox/
/// Archive) AND any user folders via the EXISTING backend read API
/// ([`crate::winlink_backend::WinlinkBackend::list_messages`] /
/// `list_user_messages` — the same path `mailbox_list` uses), tallies per-callsign
/// From + To counts, reads the operator callsign from config for self-exclusion,
/// and calls the pure [`derive_suggestions`] to exclude already-saved contacts.
///
/// **Suggest-only — NEVER auto-creates a contact.** The store is read (for the
/// existing-contact exclusion set) but never mutated.
///
/// **Degrades gracefully:** an offline / not-configured backend yields an empty
/// correspondent list (→ empty suggestions), not an error. A per-folder read
/// error is logged and skipped (best-effort) rather than failing the whole
/// command — the suggestion affordance must never block the Contacts surface.
///
/// **Lock discipline:** the `ContactsStore` mutex is locked only long enough to
/// snapshot the existing contacts, and is dropped BEFORE any `.await` on the
/// backend (mirrors the `mailbox_list` clone-and-drop invariant).
#[tauri::command]
pub async fn contacts_suggestions(
    svc: tauri::State<'_, Arc<Mutex<ContactsStore>>>,
    state: tauri::State<'_, BackendState>,
) -> Result<Vec<Suggestion>, ContactsError> {
    // Snapshot existing contacts, then drop the lock before awaiting.
    let existing: Vec<Contact> = {
        let store = svc.lock().expect("contacts store mutex poisoned");
        store.contacts().to_vec()
    };

    let op = operator_callsign();

    // Offline / not-configured backend → no correspondents (empty suggestions).
    let Some(backend) = state.current() else {
        return Ok(derive_suggestions(&[], &existing, &op));
    };

    // Enumerate every system folder + user folders; best-effort per folder.
    let mut metas: Vec<MessageMeta> = Vec::new();
    for folder in [
        MailboxFolder::Inbox,
        MailboxFolder::Sent,
        MailboxFolder::Outbox,
        MailboxFolder::Archive,
    ] {
        match backend.list_messages(folder).await {
            Ok(mut m) => metas.append(&mut m),
            Err(e) => eprintln!("contacts_suggestions: list {folder:?} failed (skipped): {e}"),
        }
    }
    match backend.list_user_folders().await {
        Ok(folders) => {
            for f in folders {
                match backend.list_user_messages(&f.slug).await {
                    Ok(mut m) => metas.append(&mut m),
                    Err(e) => eprintln!(
                        "contacts_suggestions: list user folder {} failed (skipped): {e}",
                        f.slug
                    ),
                }
            }
        }
        Err(e) => eprintln!("contacts_suggestions: list_user_folders failed (skipped): {e}"),
    }

    let correspondents = tally_correspondents(&metas);
    Ok(derive_suggestions(&correspondents, &existing, &op))
}

/// A contact's Tuxlink-native connection record, aggregated by callsign across
/// the favorites store. Carries the empirical attempt history plus the gated
/// time-of-day hint over the COMBINED set — the same render data the favorites
/// `ConnectionRecord` consumes, so contacts and favorites share one component.
///
/// snake_case wire shape — the codebase uses no `serde(rename_all)`, so the
/// field names ARE the JSON keys (`attempts`, `hint`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ContactConnectionRecord {
    /// Every recorded attempt across every favorite whose `gateway` matches the
    /// callsign, in chronological (insertion) order. Empty when no favorite
    /// matches — an honest "no connection attempts yet", never fabricated.
    pub attempts: Vec<ConnectionAttempt>,
    /// The gated time-of-day hint over the COMBINED attempts (≥3 attempts, ≥1
    /// success, strict unique max — H2), or `None`.
    pub hint: Option<TodHint>,
}

/// Surface a contact's connection record by callsign (read-only over the
/// favorites store). Reuses the favorites join: a `Favorite` keys on `gateway`
/// (the SSID-bearing station callsign), and a contact's record is the aggregate
/// of `ConnectionAttempt`s across EVERY favorite whose `gateway == callsign`.
///
/// Delegates entirely to existing favorites primitives — `attempts_for_gateway`
/// for the EXACT-match aggregation (`"W7CPZ"` does NOT match `"W7CPZ-10"`) and
/// the existing `tod_hint` gate over the combined set (offset-local bucketing
/// H1, over-claim guard H2). No new storage; nothing is mutated.
///
/// A callsign with no matching favorite yields `{ attempts: [], hint: None }` —
/// the honest empty state for a correspondent only ever reached THROUGH the CMS
/// (no direct session, hence no favorite). The card is omitted by the frontend.
#[tauri::command]
pub fn contacts_connection_record(
    callsign: String,
    favorites: tauri::State<Arc<Mutex<FavoritesStore>>>,
) -> Result<ContactConnectionRecord, ContactsError> {
    let store = favorites.lock().expect("favorites store mutex poisoned");
    let attempts = store.attempts_for_gateway(&callsign);
    let hint = crate::favorites::store::tod_hint(&attempts);
    Ok(ContactConnectionRecord { attempts, hint })
}

/// Return the gateways that this station has attempted to connect to within
/// the given look-back window (hours), suitable for rendering as pins on the
/// Winlink map layer (tuxlink-s1o1).
///
/// Delegates entirely to `FavoritesStore::recent_gateways`; `now` is supplied
/// by the real wall clock here so the store method remains deterministically
/// testable (see Task 1 tests in `favorites::store`).
///
/// The `within_hours` arg arrives from JS camelCased as `{ withinHours }`;
/// no serde rename is needed — Tauri handles JS↔Rust camelCase mapping for
/// primitive command args automatically.
#[tauri::command]
pub fn contacts_recent_gateways(
    within_hours: u32,
    favorites: tauri::State<Arc<Mutex<FavoritesStore>>>,
) -> Result<Vec<crate::favorites::store::RecentGateway>, ContactsError> {
    let store = favorites.lock().expect("favorites store mutex poisoned");
    let now = chrono::Local::now().fixed_offset();
    Ok(store.recent_gateways(within_hours, now))
}

/// The P2P integration-matrix capability flags [R5-8]. One bool per matrix
/// row. Relocated from the deleted `peers/commands.rs` (contacts-superset
/// pivot); the DTO shape and bit values are UNCHANGED here — Task T-D
/// reconciles the bit set against the pivoted surface.
///
/// **Two kinds of bit — read this before adding a query site.** Exactly THREE
/// bits are UI-QUERIED and drive the render-hide mechanism (spec R5-8: a false
/// bit HIDES its row so a half-wired feature is never operator-reachable):
/// `finder_peers`, `map_peers`, and `settings_editor`.
///
/// The other FIVE — `peer_store`, `agent_find_peers`, `agent_telnet_dial`,
/// `vara_engine_split`, and `favorites_peer_link` — are INFORMATIONAL only:
/// the agent tool / store / protocol code either exists in the binary or it
/// does not (there is no half-rendered state to hide), so nothing queries
/// them to gate rendering.
///
/// Convention: each bit starts `false` and is hardcoded `true` ONLY in the
/// task that lands its row, in that task's own commit.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct P2pCapabilities {
    /// Rows 1-2 — the roster store + the observation recorder (now the
    /// contacts superset). INFORMATIONAL.
    pub peer_store: bool,
    /// Rows 3, 5 — the roster read + Finder aggregation + filter.
    /// UI-QUERIED (hides the Finder's peers surface when false).
    pub finder_peers: bool,
    /// Row 6 — peer pins on the map layer. UI-QUERIED (hides the map layer).
    pub map_peers: bool,
    /// Row 8 — the peers settings editor. UI-QUERIED (hides the editor).
    /// The pivot cancelled the surface; T-D removes or repurposes the bit.
    pub settings_editor: bool,
    /// Row 4 — the `find_peers` agent tool. INFORMATIONAL.
    pub agent_find_peers: bool,
    /// Row 7 — the agent telnet-dial path. INFORMATIONAL. Removed by T-A
    /// (destination-trust); stays false.
    pub agent_telnet_dial: bool,
    /// Row 9 — the VARA engine split. INFORMATIONAL.
    pub vara_engine_split: bool,
    /// Row 10 — the favorites↔roster link. INFORMATIONAL.
    pub favorites_peer_link: bool,
}

/// Report the P2P integration-matrix capability bits [R5-8]. See
/// [`P2pCapabilities`] for the UI-queried-vs-informational distinction. Bit
/// VALUES are unchanged by the T-B relocation; T-D reconciles them.
#[tauri::command]
pub fn p2p_capabilities() -> P2pCapabilities {
    P2pCapabilities {
        peer_store: true,
        finder_peers: true, // Task 26 (R5-8 rows 3+5): the Finder's peers surface landed (Tasks 22-23).
        map_peers: true,    // Task 26 (R5-8 row 6): peer pins on the map layer landed (Task 24).
        settings_editor: false, // Surface cancelled by the pivot; T-D reconciles the bit itself.
        agent_find_peers: true, // Task 19 (R5-8 row 4): the find_peers agent tool (re-sourced to contacts in T-B).
        // Task 20 landed the agent telnet-dial path, then Task T-A removed it
        // (operator pivot: a telnet host:port is destination-trust the armed
        // egress gate cannot vouch for). Row 7 stays false.
        agent_telnet_dial: false,
        vara_engine_split: true, // Task 21 (R5-8 row 9): agent VARA egress dispatches on engine.
        favorites_peer_link: true, // Task 17 (R5-7): the favorites↔roster bridge landed.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contacts::reachability::{ContactTier, Origin};
    use crate::contacts::store::GroupMember;
    use crate::winlink_backend::MessageId;
    use tempfile::tempdir;

    fn meta(id: &str, from: &str, to: &[&str]) -> MessageMeta {
        MessageMeta {
            id: MessageId(id.to_string()),
            subject: "test".to_string(),
            from: from.to_string(),
            to: to.iter().map(|s| s.to_string()).collect(),
            date: "2026-06-07T12:00:00+00:00".to_string(),
            unread: false,
            body_size: 0,
            has_attachments: false,
            identity: None,
        }
    }

    fn blank_contact() -> Contact {
        Contact {
            id: String::new(),
            name: "Pat Example".to_string(),
            callsign: "W6ABC-7".to_string(),
            email: None,
            tactical: None,
            notes: None,
            tier: ContactTier::Confirmed,
            origin: Origin::Manual,
            grid: None,
            channels: vec![],
            endpoints: vec![],
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    fn blank_group() -> Group {
        Group {
            id: String::new(),
            name: "ARES Net".to_string(),
            members: vec![GroupMember::Raw {
                callsign: "KE7VAR".to_string(),
            }],
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    #[test]
    fn new_id_is_nonempty_and_unique() {
        let a = new_id();
        let b = new_id();
        assert!(!a.is_empty());
        assert!(!b.is_empty());
        assert_ne!(a, b, "uuid-v4 ids must be unique");
    }

    #[test]
    fn stamp_new_contact_sets_id_and_equal_timestamps() {
        let now = "2026-06-08T12:00:00+00:00";
        let stamped = stamp_contact(blank_contact(), now, || "fixed-id".to_string());
        assert_eq!(stamped.id, "fixed-id", "empty id must be assigned");
        assert_eq!(stamped.created_at, now);
        assert_eq!(stamped.updated_at, now);
        assert_eq!(
            stamped.created_at, stamped.updated_at,
            "new contact: created_at == updated_at"
        );
    }

    #[test]
    fn stamp_update_contact_preserves_created_at_and_id() {
        let created = "2026-06-01T08:00:00+00:00";
        let now = "2026-06-08T12:00:00+00:00";
        let mut existing = blank_contact();
        existing.id = "existing-id".to_string();
        existing.created_at = created.to_string();
        existing.updated_at = created.to_string();

        let stamped = stamp_contact(existing, now, || {
            panic!("update must NOT mint a new id")
        });
        assert_eq!(stamped.id, "existing-id", "update preserves id");
        assert_eq!(stamped.created_at, created, "update preserves created_at");
        assert_eq!(stamped.updated_at, now, "update advances updated_at");
        assert_ne!(stamped.created_at, stamped.updated_at);
    }

    #[test]
    fn stamp_new_group_sets_id_and_equal_timestamps() {
        let now = "2026-06-08T12:00:00+00:00";
        let stamped = stamp_group(blank_group(), now, || "grp-id".to_string());
        assert_eq!(stamped.id, "grp-id");
        assert_eq!(stamped.created_at, now);
        assert_eq!(stamped.updated_at, now);
    }

    #[test]
    fn stamp_update_group_preserves_created_at_and_id() {
        let created = "2026-06-01T08:00:00+00:00";
        let now = "2026-06-08T12:00:00+00:00";
        let mut existing = blank_group();
        existing.id = "grp-existing".to_string();
        existing.created_at = created.to_string();
        existing.updated_at = created.to_string();

        let stamped = stamp_group(existing, now, || panic!("update must NOT mint a new id"));
        assert_eq!(stamped.id, "grp-existing");
        assert_eq!(stamped.created_at, created);
        assert_eq!(stamped.updated_at, now);
    }

    #[test]
    fn whitespace_only_id_is_treated_as_new() {
        // A caller passing "   " (not a real id) gets a fresh id, not a
        // mistaken in-place update against a phantom record.
        let now = "2026-06-08T12:00:00+00:00";
        let mut c = blank_contact();
        c.id = "   ".to_string();
        let stamped = stamp_contact(c, now, || "minted".to_string());
        assert_eq!(stamped.id, "minted");
        assert_eq!(stamped.created_at, now);
    }

    // ------------------------------------------------------------------
    // tally_correspondents (A3) — counts From + To across message metas.
    // ------------------------------------------------------------------

    fn count_of(pairs: &[(String, u32)], who: &str) -> Option<u32> {
        pairs.iter().find(|(c, _)| c == who).map(|(_, n)| *n)
    }

    #[test]
    fn tally_counts_both_from_and_to() {
        let metas = vec![
            meta("1", "W6ABC", &["W1OP"]),
            meta("2", "KE7VAR", &["W6ABC"]),
        ];
        let pairs = tally_correspondents(&metas);
        assert_eq!(count_of(&pairs, "W6ABC"), Some(2), "From in msg1 + To in msg2");
        assert_eq!(count_of(&pairs, "W1OP"), Some(1));
        assert_eq!(count_of(&pairs, "KE7VAR"), Some(1));
    }

    #[test]
    fn tally_dedups_within_a_single_message() {
        // A callsign listed twice in one message's To counts once for that msg.
        let metas = vec![meta("1", "W6ABC", &["KE7VAR", "ke7var", "KE7VAR"])];
        let pairs = tally_correspondents(&metas);
        assert_eq!(count_of(&pairs, "KE7VAR"), Some(1), "deduped per message");
        assert_eq!(count_of(&pairs, "W6ABC"), Some(1));
    }

    #[test]
    fn tally_skips_blank_addresses() {
        let metas = vec![meta("1", "  ", &["", "W6ABC"])];
        let pairs = tally_correspondents(&metas);
        assert_eq!(pairs.len(), 1);
        assert_eq!(count_of(&pairs, "W6ABC"), Some(1));
    }

    #[test]
    fn tally_empty_input_is_empty() {
        assert!(tally_correspondents(&[]).is_empty());
    }

    // ------------------------------------------------------------------
    // contacts_connection_record (tuxlink-je5d) — the by-callsign join.
    //
    // The `#[tauri::command]` wrapper only adds `State` extraction; we exercise
    // the command BODY against a raw `FavoritesStore` so the assembly
    // (attempts_for_gateway → tod_hint → ContactConnectionRecord) is tested
    // without a Tauri harness, mirroring favorites' tod_hint delegation test.
    // ------------------------------------------------------------------

    /// Replicate the `contacts_connection_record` body against a raw store.
    fn connection_record_via_command_logic(
        store: &FavoritesStore,
        callsign: &str,
    ) -> ContactConnectionRecord {
        let attempts = store.attempts_for_gateway(callsign);
        let hint = crate::favorites::store::tod_hint(&attempts);
        ContactConnectionRecord { attempts, hint }
    }

    fn dial(mode: &str, gateway: &str, freq: &str) -> crate::favorites::store::FavoriteDial {
        crate::favorites::store::FavoriteDial {
            mode: mode.to_string(),
            gateway: gateway.to_string(),
            freq: Some(freq.to_string()),
            transport: None,
            band: None,
            grid: None,
            contact_id: None,
        }
    }

    #[test]
    fn connection_record_aggregates_by_callsign_with_hint() {
        // A callsign spanning two favorites (same gateway, different mode/freq)
        // aggregates both attempt streams and runs tod_hint over the union.
        let dir = tempdir().unwrap();
        let mut store = FavoritesStore::open(dir.path().join("stations.json"));
        for (i, (mode, freq, ts)) in [
            ("ardop-hf", "14105.0", "2026-06-07T23:00:00-07:00"),
            ("ardop-hf", "14105.0", "2026-06-07T22:00:00-07:00"),
            ("vara-hf", "7102.0", "2026-06-07T21:00:00-07:00"),
        ]
        .into_iter()
        .enumerate()
        {
            // record_attempt's new_id is FnOnce (called only when a new favorite is
            // created); a fresh per-call closure avoids sharing one across the loop.
            let id = format!("u{}", i + 1);
            store
                .record_attempt(
                    dial(mode, "W7CPZ", freq),
                    "reached".to_string(),
                    ts.to_string(),
                    move || id,
                    "2026-06-08T06:00:00+00:00".to_string(),
                )
                .unwrap();
        }

        let record = connection_record_via_command_logic(&store, "W7CPZ");
        assert_eq!(record.attempts.len(), 3, "attempts from both favorites aggregate");
        let hint = record.hint.expect("3 night successes → Some");
        assert_eq!(hint.bucket, "night");
        assert_eq!(hint.successes, 3);
    }

    #[test]
    fn connection_record_empty_for_unmatched_callsign() {
        // No matching favorite → honest empty state: empty attempts + None hint.
        let dir = tempdir().unwrap();
        let store = FavoritesStore::open(dir.path().join("stations.json"));
        let record = connection_record_via_command_logic(&store, "W7CPZ");
        assert!(record.attempts.is_empty(), "no favorite → empty attempts");
        assert!(record.hint.is_none(), "no favorite → None hint (never fabricated)");
    }

    // ------------------------------------------------------------------
    // contacts_recent_gateways (tuxlink-s1o1) — JSON shape on the wire.
    //
    // The `#[tauri::command]` wrapper adds only `State` extraction and the
    // real wall clock; `FavoritesStore::recent_gateways` is fully exercised
    // by Task 1's unit tests.  This test asserts the serialization contract
    // the frontend depends on: keys are snake_case on the wire (Tauri
    // serializes via serde_json; no rename_all on the struct means the
    // Rust field names ARE the wire names, which are already snake_case).
    // ------------------------------------------------------------------

    #[test]
    fn recent_gateways_serializes_snake_case() {
        let rg = crate::favorites::store::RecentGateway {
            gateway: "W6DRZ".into(),
            grid: Some("CM97".into()),
            last_attempt_at: "2026-06-22T11:30:00-07:00".into(),
            outcome: "reached".into(),
        };
        let v = serde_json::to_value(&rg).unwrap();
        assert!(v.get("last_attempt_at").is_some(), "snake_case on the wire");
        assert!(v.get("lastAttemptAt").is_none());
    }

    // ------------------------------------------------------------------
    // merge_for_upsert — the tier + reachability preservation contract
    // (spec §AMENDMENT pt. 1: upsert never silently flips tier).
    // ------------------------------------------------------------------

    #[test]
    fn upsert_new_record_is_confirmed_and_added() {
        let merged = merge_for_upsert(None, blank_contact());
        assert_eq!(merged.tier, ContactTier::Confirmed, "operator-created → curated tier");
        assert_eq!(merged.origin, Origin::Manual, "operator-created → added");
    }

    #[test]
    fn upsert_does_not_flip_tier_and_preserves_reachability() {
        use crate::contacts::reachability::*;
        // The STORED record: unconfirmed, with an observed channel + endpoint.
        let mut stored = blank_contact();
        stored.id = "c1".to_string();
        stored.tier = ContactTier::Unconfirmed;
        stored.origin = Origin::Incoming;
        stored.channels = vec![Channel {
            transport: ChannelTransport::VaraHf,
            target_callsign: "W6ABC-7".into(),
            via: vec![],
            freq_hz: Some(7_101_000),
            bandwidth: None,
            direction: Direction::Incoming,
            counts: AttemptCounts { ok: 1, fail: 0 },
            last_seen: "2026-07-11T12:00:00-07:00".into(),
        }];
        stored.endpoints = vec![Endpoint {
            id: "e1".into(),
            host: "203.0.113.5".into(),
            port: 8772,
            provenance: Provenance::ObservedIncoming,
            last_seen: "2026-07-11T12:00:00-07:00".into(),
        }];
        // The editor's snapshot: identity edits, EMPTY reachability, and a
        // (stale/hostile) Confirmed tier + Manual origin.
        let mut incoming = blank_contact();
        incoming.id = "c1".to_string();
        incoming.name = "Pat Renamed".to_string();
        incoming.tier = ContactTier::Confirmed;
        incoming.origin = Origin::Manual;

        let merged = merge_for_upsert(Some(&stored), incoming);
        assert_eq!(merged.name, "Pat Renamed", "identity edits land");
        assert_eq!(merged.tier, ContactTier::Unconfirmed, "upsert must not flip tier");
        assert_eq!(merged.origin, Origin::Incoming, "origin preserved");
        assert_eq!(merged.channels.len(), 1, "observed channels preserved");
        assert_eq!(merged.endpoints.len(), 1, "observed endpoints preserved");
    }

    // ------------------------------------------------------------------
    // endpoint_password_set_guard — the [R2-S7] attach boundary.
    // ------------------------------------------------------------------

    fn file_with_endpoint(provenance: crate::contacts::reachability::Provenance) -> ContactsFile {
        use crate::contacts::reachability::Endpoint;
        let mut c = blank_contact();
        c.id = "c1".to_string();
        c.endpoints = vec![Endpoint {
            id: "e1".into(),
            host: "peer.example.org".into(),
            port: 8772,
            provenance,
            last_seen: "2026-07-11T12:00:00-07:00".into(),
        }];
        let mut f = ContactsFile::default();
        f.contacts.push(c);
        f
    }

    #[test]
    fn password_set_guard_allows_operator_endpoints_only() {
        use crate::contacts::reachability::Provenance;
        let operator = file_with_endpoint(Provenance::Operator);
        assert!(endpoint_password_set_guard(&operator, "c1", "e1").is_ok());

        // [R2-S7]: a password may never attach to an observed endpoint.
        let observed = file_with_endpoint(Provenance::ObservedIncoming);
        assert!(matches!(
            endpoint_password_set_guard(&observed, "c1", "e1"),
            Err(ContactsError::Validation(_))
        ));

        // A non-existent pair is refused (never orphan a fresh secret).
        assert!(matches!(
            endpoint_password_set_guard(&operator, "c1", "ghost"),
            Err(ContactsError::Validation(_))
        ));
        assert!(matches!(
            endpoint_password_set_guard(&operator, "ghost", "e1"),
            Err(ContactsError::Validation(_))
        ));
    }

    // ------------------------------------------------------------------
    // p2p_capabilities — relocated verbatim from the deleted peers module;
    // bit values unchanged (T-D reconciles the set).
    // ------------------------------------------------------------------

    #[test]
    fn capabilities_report_only_landed_rows_true() {
        let c = p2p_capabilities();
        assert!(c.peer_store, "store + recorder landed (now the contacts superset)");
        assert!(c.finder_peers, "Task 26 landed the Finder's peers surface");
        assert!(c.map_peers, "Task 26 landed the map layer's peer pins");
        assert!(!c.settings_editor, "surface cancelled by the pivot; never landed");
        assert!(c.agent_find_peers, "find_peers agent tool (re-sourced to contacts)");
        assert!(
            !c.agent_telnet_dial,
            "Task 20's agent telnet-dial path was removed by Task T-A"
        );
        assert!(c.vara_engine_split, "Task 21 landed the VARA engine split");
        assert!(c.favorites_peer_link, "Task 17 landed the favorites bridge");
    }

    #[test]
    fn capabilities_serialize_camelless_snake_case_on_the_wire() {
        // The struct carries no serde(rename_all), so the field names ARE the
        // wire keys — the contract the UI query sites read. Pin it.
        let v = serde_json::to_value(p2p_capabilities()).unwrap();
        assert_eq!(v.get("peer_store").and_then(|b| b.as_bool()), Some(true));
        assert_eq!(v.get("finder_peers").and_then(|b| b.as_bool()), Some(true));
        assert_eq!(v.get("settings_editor").and_then(|b| b.as_bool()), Some(false));
        assert!(v.get("favorites_peer_link").is_some());
    }
}
