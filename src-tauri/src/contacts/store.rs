//! Contacts JSON store — the `contacts.json` address book backing the Contacts
//! feature (Compose autocomplete + the Contacts surface build on it).
//!
//! Plan: docs/superpowers/plans/2026-06-07-contacts-favorites.md → "Locked
//! decisions" + "Task A1". Hardened by a 5-round adversarial review; the
//! data-loss invariants below are load-bearing, not stylistic.
//!
//! **Forward-compat + data-loss policy (C1/M1):**
//! - `#[serde(default)]` additive tolerance on `ContactsFile` fields; NO
//!   `#[serde(deny_unknown_fields)]` (it would reject forward-version files and
//!   trigger data loss). Unknown future fields are silently ignored.
//! - [`ContactsStore::open`] is INFALLIBLE — it always returns a usable store.
//!   On ANY full parse/read failure it FIRST renames the unreadable file to
//!   `<name>.corrupt-<utc-ts>` (preserving the original bytes), `eprintln!`s a
//!   warning, THEN returns the default empty store. Never blocks startup; never
//!   silently overwrites a corrupt file's bytes with empty JSON. (Mirrors the
//!   DEGRADE-on-error pattern of `user_folders.rs::load_registry`, extended
//!   with corrupt-file preservation.)
//! - Hand-written `impl Default for ContactsFile` sets `schema_version:
//!   SCHEMA_VERSION`; NO `#[derive(Default)]` (it would write 0).
//!
//! **Atomic write:** serialize → write to `format!("{}.tmp", path_str)` (NOT
//! `path.with_extension("tmp")`, which would drop `.json`) → `fs::rename`;
//! `create_dir_all(parent)` first. Mirrors `user_folders.rs:182-192`.

use super::reachability::{
    AttemptCounts, Channel, ContactGrid, ContactTier, Direction, Endpoint, Origin, Provenance,
    AUTO_CONTACT_CAP,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;

/// Max `channels` per contact. Rotated observations (via/freq churn) on ONE
/// contact must not grow it without bound; oldest-`last_seen` is evicted
/// (R3-F4, carried over from the peers store).
const PER_CONTACT_CHANNEL_CAP: usize = 64;

/// On-disk schema version.
///
/// - v1: the 2026-06-07 Contacts+Favorites shape (identity + mail fields).
/// - v2: the contacts-superset pivot (spec §AMENDMENT, 2026-07-10/11) —
///   Contact gains `tier` / `origin` / `grid` / `channels` / `endpoints`.
///   Loading a v1 file (or one with no `schema_version` at all) yields v2
///   semantics via the serde defaults: every existing record is
///   `tier: Confirmed` with empty reachability. [`ContactsStore::open`]
///   normalizes the in-memory version to v2 so the next flush stamps it.
pub const SCHEMA_VERSION: u32 = 2;

/// A single address-book contact — since schema v2 the SUPERSET of added +
/// observed stations (spec §AMENDMENT pt. 1). The callsign is the SSID-bearing
/// primary identity — NEVER strip the SSID; observation routing matches on the
/// EXACT presented callsign only (pt. 4 — no base-normalization merging).
/// Timestamps are RFC3339 UTC on the operator path; id/timestamp STAMPING is
/// the command layer's job (Task A2), not the store's.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Contact {
    pub id: String,
    pub name: String,
    pub callsign: String,
    pub email: Option<String>,
    pub tactical: Option<String>,
    pub notes: Option<String>,
    /// `Confirmed` (curated — operator added/confirmed) vs `Unconfirmed`
    /// (auto-created from an observation or manual dial). Default `Confirmed`
    /// so every v1 record loads as confirmed (the v1→v2 migration).
    #[serde(default)]
    pub tier: ContactTier,
    /// Plain-language provenance of the record: incoming / outgoing / added.
    #[serde(default)]
    pub origin: Origin,
    #[serde(default)]
    pub grid: Option<ContactGrid>,
    /// Observed RF reachability rows (spec §2 shapes, unchanged by the pivot).
    #[serde(default)]
    pub channels: Vec<Channel>,
    /// Observed / operator-entered network reachability rows (telnet P2P).
    #[serde(default)]
    pub endpoints: Vec<Endpoint>,
    pub created_at: String,
    pub updated_at: String,
}

/// A distribution-group member. Stored as a `contact_id` reference when added
/// from a contact (so edits propagate), or a raw literal callsign when typed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum GroupMember {
    Contact { contact_id: String },
    Raw { callsign: String },
}

/// A distribution group (named set of members).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Group {
    pub id: String,
    pub name: String,
    pub members: Vec<GroupMember>,
    pub created_at: String,
    pub updated_at: String,
}

/// The on-disk file shape. `#[serde(default)]` on every field gives additive
/// forward-compat tolerance; there is deliberately NO `deny_unknown_fields`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContactsFile {
    #[serde(default)]
    pub schema_version: u32,
    #[serde(default)]
    pub contacts: Vec<Contact>,
    #[serde(default)]
    pub groups: Vec<Group>,
}

// NO derive(Default) (M1) — hand-write Default so schema_version is 1, not 0.
impl Default for ContactsFile {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            contacts: vec![],
            groups: vec![],
        }
    }
}

/// Serializable error projection for the IPC boundary. Mirrors the
/// `#[serde(tag = "kind", content = "detail")]` discriminated-union shape used
/// by `ui_commands.rs::UiError` so the frontend gets a `{ kind, detail }` shape.
#[derive(Debug, Error, Serialize)]
#[serde(tag = "kind", content = "detail")]
pub enum ContactsError {
    #[error("io: {0}")]
    Io(String),
    #[error("serde: {0}")]
    Serde(String),
    #[error("validation: {0}")]
    Validation(String),
}

/// The roster effect of a [`ContactsStore::apply_observation`] call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplyEffect {
    /// The observation created a new `Unconfirmed` contact.
    CreatedContact,
    /// The observation attached to an existing contact (any tier).
    UpdatedContact,
    /// A rejected/unauthorized inbound or an invalid callsign — no write.
    NoRecord,
}

/// The full outcome of a [`ContactsStore::apply_observation`] call: the roster
/// effect plus the `(contact_id, endpoint_id)` pairs whose contacts were
/// LRU-evicted over [`AUTO_CONTACT_CAP`] — the caller cascades their keyring
/// secrets (spec §AMENDMENT pt. 8; the store never touches the keyring).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyOutcome {
    pub effect: ApplyEffect,
    pub evicted_endpoint_secrets: Vec<(String, String)>,
}

impl ApplyOutcome {
    /// The no-write outcome (rejected inbound, invalid callsign, or a store
    /// failure surfaced by the caller).
    pub fn no_record() -> Self {
        Self {
            effect: ApplyEffect::NoRecord,
            evicted_endpoint_secrets: vec![],
        }
    }
}

/// Resolve the SINGLE `(contact_id, endpoint_id)` pair a legacy
/// `p2p-peer:<CALLSIGN>` keyring secret may be conservatively re-keyed to
/// [R5-5]: exactly one contact whose callsign matches `callsign` exactly
/// (case-insensitive, no base normalization) AND exactly one
/// `Provenance::Operator` endpoint on it. Any ambiguity — zero or multiple
/// matching contacts, zero or multiple operator endpoints — returns `None`,
/// and the caller leaves the legacy secret in place (the manual-reassignment
/// signal is the legacy key still answering by callsign).
pub fn unambiguous_operator_endpoint(
    file: &ContactsFile,
    callsign: &str,
) -> Option<(String, String)> {
    let wanted = callsign.trim();
    if wanted.is_empty() {
        return None;
    }
    let mut matches = file
        .contacts
        .iter()
        .filter(|c| c.callsign.trim().eq_ignore_ascii_case(wanted));
    let only = matches.next()?;
    if matches.next().is_some() {
        return None; // multiple contacts on this callsign — ambiguous
    }
    let mut ops = only
        .endpoints
        .iter()
        .filter(|e| e.provenance == Provenance::Operator);
    let ep = ops.next()?;
    if ops.next().is_some() {
        return None; // multiple operator endpoints — ambiguous
    }
    Some((only.id.clone(), ep.id.clone()))
}

/// The contacts store: an in-memory [`ContactsFile`] plus the path it persists
/// to. Mutations flush eagerly. Construct via [`ContactsStore::open`].
pub struct ContactsStore {
    path: PathBuf,
    file: ContactsFile,
}

impl ContactsStore {
    /// Open the store at `path`. INFALLIBLE — always returns a usable store.
    ///
    /// - Missing file → default empty store.
    /// - Present + parseable → the parsed file.
    /// - Present + UNparseable (read error or JSON error): rename the file to
    ///   `<name>.corrupt-<utc-ts>` to PRESERVE the original bytes, `eprintln!`
    ///   a warning, then return the default empty store. The corrupt original
    ///   is never overwritten in place; a later flush writes only to `path`,
    ///   leaving the sidecar intact.
    pub fn open(path: PathBuf) -> Self {
        let file = match std::fs::read(&path) {
            Ok(bytes) => match serde_json::from_slice::<ContactsFile>(&bytes) {
                Ok(mut parsed) => {
                    // v1→v2 migration is the serde defaults themselves (every
                    // pre-tier record loads `Confirmed` with empty
                    // reachability); normalize the version stamp so the next
                    // flush writes v2.
                    parsed.schema_version = SCHEMA_VERSION;
                    parsed
                }
                Err(e) => {
                    Self::quarantine_corrupt(&path, &bytes);
                    eprintln!(
                        "contacts: {} is unparseable, starting empty (original preserved): {e}",
                        path.display()
                    );
                    ContactsFile::default()
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => ContactsFile::default(),
            Err(e) => {
                // A non-NotFound read error (e.g. permission/partial). Try to
                // preserve whatever bytes we can read; if even that fails, we
                // still degrade to empty rather than blocking startup.
                if let Ok(bytes) = std::fs::read(&path) {
                    Self::quarantine_corrupt(&path, &bytes);
                }
                eprintln!(
                    "contacts: failed to read {}, starting empty: {e}",
                    path.display()
                );
                ContactsFile::default()
            }
        };
        Self { path, file }
    }

    /// Rename the unreadable file to a timestamped `.corrupt-*` sidecar,
    /// preserving the original bytes. Falls back to a copy-write if the rename
    /// itself fails (best-effort preservation; never panics).
    fn quarantine_corrupt(path: &std::path::Path, original: &[u8]) {
        let ts = chrono::Utc::now().format("%Y%m%dT%H%M%SZ");
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "contacts.json".to_string());
        let corrupt = path.with_file_name(format!("{name}.corrupt-{ts}"));
        if let Err(e) = std::fs::rename(path, &corrupt) {
            // Rename failed (e.g. cross-device); fall back to copying the bytes
            // out so the original is not lost when a later flush overwrites it.
            eprintln!(
                "contacts: could not rename corrupt {} → {} ({e}); copying bytes instead",
                path.display(),
                corrupt.display()
            );
            let _ = std::fs::write(&corrupt, original);
        }
    }

    /// Persist the in-memory file atomically: serialize → write to a sibling
    /// `<name>.tmp` → `rename` over the final path. `create_dir_all(parent)`
    /// first. Uses `format!("{}.tmp", name)` so the suffix is `contacts.json.tmp`
    /// (NOT `with_extension("tmp")`, which would drop `.json`).
    fn flush(&self) -> Result<(), ContactsError> {
        let json = serde_json::to_string_pretty(&self.file)
            .map_err(|e| ContactsError::Serde(e.to_string()))?;
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| ContactsError::Io(e.to_string()))?;
        }
        let name = self
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "contacts.json".to_string());
        let tmp = self.path.with_file_name(format!("{name}.tmp"));
        std::fs::write(&tmp, json).map_err(|e| ContactsError::Io(e.to_string()))?;
        std::fs::rename(&tmp, &self.path).map_err(|e| ContactsError::Io(e.to_string()))?;
        Ok(())
    }

    /// All contacts (read-only view).
    pub fn contacts(&self) -> &[Contact] {
        &self.file.contacts
    }

    /// All groups (read-only view).
    pub fn groups(&self) -> &[Group] {
        &self.file.groups
    }

    /// The whole in-memory file (read-only view) — used by the `contacts_read`
    /// command (Task A2) to return the full DTO.
    pub fn file(&self) -> &ContactsFile {
        &self.file
    }

    /// Insert a contact, or replace the existing one with the same `id`. The
    /// store takes the contact as given (id/timestamp stamping is the command
    /// layer's job, Task A2). Flushes on success.
    pub fn contact_upsert(&mut self, c: Contact) -> Result<(), ContactsError> {
        match self.file.contacts.iter_mut().find(|x| x.id == c.id) {
            Some(existing) => *existing = c,
            None => self.file.contacts.push(c),
        }
        self.flush()
    }

    /// Remove a contact by id (no-op if absent). Returns the removed contact's
    /// endpoint ids so the `contact_delete` command can cascade the id-keyed
    /// keyring secrets [R2-S7] — no roster mutation ever orphans a stored
    /// password. Flushes when a record was actually removed.
    pub fn contact_delete(&mut self, id: &str) -> Result<Vec<String>, ContactsError> {
        match self.file.contacts.iter().position(|c| c.id == id) {
            Some(idx) => {
                let removed = self.file.contacts.remove(idx);
                let endpoint_ids = removed.endpoints.iter().map(|e| e.id.clone()).collect();
                self.flush()?;
                Ok(endpoint_ids)
            }
            None => Ok(vec![]),
        }
    }

    /// Flip a contact's tier `Unconfirmed → Confirmed` — the one-click promote
    /// (spec §AMENDMENT pt. 7: "Promote = one-click add"). Idempotent on an
    /// already-confirmed record. Errors when the id is unknown. Flushes.
    pub fn contact_confirm(&mut self, id: &str, now: String) -> Result<(), ContactsError> {
        let c = self
            .file
            .contacts
            .iter_mut()
            .find(|c| c.id == id)
            .ok_or_else(|| ContactsError::Validation(format!("contact {id:?} not found")))?;
        c.tier = ContactTier::Confirmed;
        c.updated_at = now;
        self.flush()
    }

    /// Insert a group, or replace the existing one with the same `id`. Flushes.
    pub fn group_upsert(&mut self, g: Group) -> Result<(), ContactsError> {
        match self.file.groups.iter_mut().find(|x| x.id == g.id) {
            Some(existing) => *existing = g,
            None => self.file.groups.push(g),
        }
        self.flush()
    }

    /// Remove a group by id (no-op if absent). Flushes on success.
    pub fn group_delete(&mut self, id: &str) -> Result<(), ContactsError> {
        self.file.groups.retain(|g| g.id != id);
        self.flush()
    }

    /// Route a concluded connection observation to its contact record and
    /// apply it (spec §3 + §AMENDMENT pts. 1/4/8).
    ///
    /// **Exact presented-callsign match only** (pt. 4): the observation
    /// attaches to the contact whose callsign equals the presented form
    /// exactly (case-insensitive; ANY tier — a confirmed address-book entry
    /// accrues reachability like an unconfirmed one). No base-normalization
    /// merging: `W6ABC-7` and `W6ABC` are distinct records. Otherwise it
    /// creates an `Unconfirmed` contact (name empty, callsign = the exact
    /// presented form, origin from the observation's direction).
    ///
    /// The caller (the recorder) has already classified rejected-inbound via
    /// `observation::classify`; the `NoRecord` re-check here is
    /// defense-in-depth — a `NoRecord` phase is never a roster write. The
    /// write boundary gates on
    /// [`crate::winlink::callsign::validate_presented_callsign`] (NOT
    /// `sanitize_display`) so a legitimate portable form like `W6ABC/P` is
    /// stored rather than dropped [R3-F2].
    ///
    /// Caps guard `Unconfirmed` ONLY (pt. 8): creation may LRU-evict the
    /// stalest unconfirmed record over [`AUTO_CONTACT_CAP`] (`Confirmed` is
    /// never evicted); the returned [`ApplyOutcome`] carries the evicted
    /// `(contact_id, endpoint_id)` pairs for the caller's keyring cascade.
    pub fn apply_observation(
        &mut self,
        obs: &crate::contacts::observation::PeerObservation,
        now: String,
    ) -> Result<ApplyOutcome, ContactsError> {
        use crate::contacts::observation::{classify, Classified, ObservedPath};
        let bucket = classify(obs.phase);
        if matches!(bucket, Classified::NoRecord) {
            return Ok(ApplyOutcome::no_record());
        }
        let presented = obs.presented_target.trim().to_ascii_uppercase();
        // Write-boundary floor [R3-F2]: gate on the PRESENTED validator, not
        // `sanitize_display`, so a legit portable form (`W6ABC/P`) is stored.
        if crate::winlink::callsign::validate_presented_callsign(&presented).is_err() {
            return Ok(ApplyOutcome::no_record());
        }

        // ── Routing (§AMENDMENT pt. 4): exact presented-callsign only ───────
        let idx = self
            .file
            .contacts
            .iter()
            .position(|c| c.callsign.trim().eq_ignore_ascii_case(&presented));
        let created = idx.is_none();
        let idx = match idx {
            Some(i) => i,
            None => {
                self.file.contacts.push(Contact {
                    id: uuid::Uuid::new_v4().to_string(),
                    name: String::new(),
                    callsign: presented.clone(),
                    email: None,
                    tactical: None,
                    notes: None,
                    // Auto-creation NEVER lands in the curated tier (pt. 1).
                    tier: ContactTier::Unconfirmed,
                    origin: match obs.direction {
                        Direction::Incoming => Origin::Incoming,
                        Direction::Outgoing => Origin::Outgoing,
                        Direction::Unknown => Origin::Unknown,
                    },
                    grid: None,
                    channels: vec![],
                    endpoints: vec![],
                    created_at: now.clone(),
                    updated_at: now.clone(),
                });
                self.file.contacts.len() - 1
            }
        };

        // ── Apply the observation to the record ──────────────────────────────
        let ok = matches!(bucket, Classified::Ok);
        {
            let c = &mut self.file.contacts[idx];
            c.updated_at = now.clone();
            match &obs.path {
                ObservedPath::Rf {
                    transport,
                    via,
                    freq_hz,
                    bandwidth,
                } => {
                    // Channel dedup key: (transport, target_callsign, via,
                    // freq_hz exact, bandwidth) [R4-11].
                    let key_match = |ch: &Channel| {
                        ch.transport == *transport
                            && ch.target_callsign == presented
                            && ch.via == *via
                            && ch.freq_hz == *freq_hz
                            && ch.bandwidth == *bandwidth
                    };
                    if let Some(ch) = c.channels.iter_mut().find(|ch| key_match(ch)) {
                        if ok {
                            ch.counts.ok = ch.counts.ok.saturating_add(1);
                            // Success-only recency (T-F Part 0): only an OK bumps
                            // last_ok, and its direction is captured ATOMICALLY
                            // with it — `ch.direction` below mutates on failures
                            // too, so it cannot truthfully flavor the success verb.
                            ch.last_ok = Some(now.clone());
                            ch.last_ok_direction = Some(obs.direction);
                        } else {
                            ch.counts.fail = ch.counts.fail.saturating_add(1);
                        }
                        ch.direction = obs.direction;
                        ch.last_seen = now.clone();
                    } else {
                        c.channels.push(Channel {
                            transport: *transport,
                            target_callsign: presented.clone(),
                            via: via.clone(),
                            freq_hz: *freq_hz,
                            bandwidth: *bandwidth,
                            direction: obs.direction,
                            counts: AttemptCounts {
                                ok: u32::from(ok),
                                fail: u32::from(!ok),
                            },
                            last_seen: now.clone(),
                            // last_ok (+ its direction) is set ONLY on a
                            // successful first attempt, atomically.
                            last_ok: if ok { Some(now.clone()) } else { None },
                            last_ok_direction: if ok { Some(obs.direction) } else { None },
                        });
                    }
                }
                ObservedPath::Telnet {
                    host,
                    port,
                    provenance,
                } => {
                    // Monotonic provenance [R4-4]: an INBOUND observation NEVER
                    // creates or mutates an `Operator` endpoint (the downgrade
                    // is conditioned on direction == Incoming — an outbound
                    // operator dial legitimately records `Operator`; that is
                    // how a password-bearing endpoint is born).
                    let prov = if *provenance == Provenance::Operator
                        && obs.direction == Direction::Incoming
                    {
                        Provenance::ObservedIncoming
                    } else {
                        *provenance
                    };
                    let hostn = host.trim().to_ascii_lowercase();
                    // Endpoint dedup key: (host_normalized, port, provenance).
                    if let Some(ep) = c
                        .endpoints
                        .iter_mut()
                        .find(|e| e.host == hostn && e.port == *port && e.provenance == prov)
                    {
                        ep.last_seen = now.clone();
                        if ok {
                            // Success-only recency (T-F Part 0): a fail never
                            // bumps last_ok, so a "reached" label stays honest.
                            ep.last_ok = Some(now.clone());
                        }
                    } else {
                        c.endpoints.push(Endpoint {
                            id: uuid::Uuid::new_v4().to_string(),
                            host: hostn,
                            port: *port,
                            provenance: prov,
                            last_seen: now.clone(),
                            last_ok: if ok { Some(now.clone()) } else { None },
                        });
                    }
                }
            }
            Self::enforce_channel_cap(c);
        }

        // Cap check only on creation — updates cannot grow the roster.
        let evicted = if created {
            self.evict_unconfirmed_over(AUTO_CONTACT_CAP)
        } else {
            vec![]
        };
        self.flush()?;
        Ok(ApplyOutcome {
            effect: if created {
                ApplyEffect::CreatedContact
            } else {
                ApplyEffect::UpdatedContact
            },
            evicted_endpoint_secrets: evicted,
        })
    }

    /// A contact's LRU key for unconfirmed-cap eviction: the most recent
    /// `last_seen` across its channels + endpoints, falling back to
    /// `created_at` (a never-connected record ages from its creation).
    fn last_seen_key(c: &Contact) -> String {
        c.channels
            .iter()
            .map(|ch| ch.last_seen.as_str())
            .chain(c.endpoints.iter().map(|e| e.last_seen.as_str()))
            .max()
            .unwrap_or(c.created_at.as_str())
            .to_string()
    }

    /// LRU eviction among `Unconfirmed` records only (spec §AMENDMENT pt. 8):
    /// `Confirmed` contacts are never auto-created and never evicted. Returns
    /// the evicted records' `(contact_id, endpoint_id)` pairs so the caller
    /// can cascade their keyring secrets — the store never touches the
    /// keyring itself. Does NOT flush (the caller flushes once).
    fn evict_unconfirmed_over(&mut self, cap: usize) -> Vec<(String, String)> {
        let mut evicted: Vec<(String, String)> = vec![];
        loop {
            let unconfirmed: Vec<usize> = self
                .file
                .contacts
                .iter()
                .enumerate()
                .filter(|(_, c)| c.tier == ContactTier::Unconfirmed)
                .map(|(i, _)| i)
                .collect();
            if unconfirmed.len() <= cap {
                return evicted;
            }
            let lru = unconfirmed
                .into_iter()
                .min_by(|&a, &b| {
                    Self::last_seen_key(&self.file.contacts[a])
                        .cmp(&Self::last_seen_key(&self.file.contacts[b]))
                })
                .expect("non-empty by the cap check");
            let victim = self.file.contacts.remove(lru);
            for e in &victim.endpoints {
                evicted.push((victim.id.clone(), e.id.clone()));
            }
        }
    }

    /// Per-contact channel bounding (R3-F4): rotated observations (via/freq
    /// churn) on one contact cannot grow it without bound; oldest-`last_seen`
    /// is evicted.
    fn enforce_channel_cap(c: &mut Contact) {
        while c.channels.len() > PER_CONTACT_CHANNEL_CAP {
            let victim = c
                .channels
                .iter()
                .enumerate()
                .min_by(|(_, a), (_, b)| a.last_seen.cmp(&b.last_seen))
                .map(|(i, _)| i);
            match victim {
                Some(i) => {
                    c.channels.remove(i);
                }
                None => break,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn contact(id: &str) -> Contact {
        Contact {
            id: id.to_string(),
            name: "Pat Example".to_string(),
            callsign: "W6ABC-7".to_string(),
            email: Some("w6abc@winlink.org".to_string()),
            tactical: None,
            notes: None,
            tier: ContactTier::Confirmed,
            origin: Origin::Manual,
            grid: None,
            channels: vec![],
            endpoints: vec![],
            created_at: "2026-06-07T12:00:00+00:00".to_string(),
            updated_at: "2026-06-07T12:00:00+00:00".to_string(),
        }
    }

    fn group(id: &str) -> Group {
        Group {
            id: id.to_string(),
            name: "ARES Net".to_string(),
            members: vec![
                GroupMember::Contact { contact_id: "c1".to_string() },
                GroupMember::Raw { callsign: "KE7VAR".to_string() },
            ],
            created_at: "2026-06-07T12:00:00+00:00".to_string(),
            updated_at: "2026-06-07T12:00:00+00:00".to_string(),
        }
    }

    #[test]
    fn open_missing_returns_empty() {
        let dir = tempdir().unwrap();
        let store = ContactsStore::open(dir.path().join("contacts.json"));
        assert_eq!(store.file().schema_version, SCHEMA_VERSION);
        assert!(store.contacts().is_empty());
        assert!(store.groups().is_empty());
    }

    #[test]
    fn fresh_empty_store_has_current_schema_version() {
        // M1: a brand-new store written to disk persists schema_version:2,
        // NOT 0 (guards against an accidental derive(Default)).
        let dir = tempdir().unwrap();
        let path = dir.path().join("contacts.json");
        let mut store = ContactsStore::open(path.clone());
        // A mutation triggers a flush, writing the file with the default version.
        store.contact_upsert(contact("c1")).unwrap();
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(
            raw.contains("\"schema_version\": 2"),
            "expected schema_version 2 on disk, got: {raw}"
        );
        // And reopening yields version 2.
        let reopened = ContactsStore::open(path);
        assert_eq!(reopened.file().schema_version, 2);
    }

    #[test]
    fn v1_file_migrates_to_v2_semantics() {
        // The v1→v2 migration (spec §AMENDMENT pt. 1): a LITERAL v1 fixture —
        // written by the shipped 2026-06-07 binary, no tier/origin/grid/
        // channels/endpoints keys anywhere — loads with every record
        // `tier: Confirmed`, empty reachability, and the in-memory version
        // normalized to v2 so the next flush stamps schema_version 2.
        let dir = tempdir().unwrap();
        let path = dir.path().join("contacts.json");
        let v1 = r#"{
            "schema_version": 1,
            "contacts": [
                {
                    "id": "c1", "name": "Pat", "callsign": "W6ABC-7",
                    "email": "w6abc@winlink.org", "tactical": null, "notes": null,
                    "created_at": "2026-06-07T12:00:00+00:00",
                    "updated_at": "2026-06-07T12:00:00+00:00"
                }
            ],
            "groups": []
        }"#;
        std::fs::write(&path, v1).unwrap();

        let mut store = ContactsStore::open(path.clone());
        assert_eq!(store.file().schema_version, SCHEMA_VERSION, "normalized to v2");
        assert_eq!(store.contacts().len(), 1, "v1 records all survive");
        let c = &store.contacts()[0];
        assert_eq!(c.tier, ContactTier::Confirmed, "existing records → confirmed");
        assert_eq!(c.origin, Origin::Unknown, "no fabricated provenance");
        assert!(c.channels.is_empty(), "empty reachability");
        assert!(c.endpoints.is_empty(), "empty reachability");
        assert!(c.grid.is_none());
        // No quarantine sidecar — a v1 file is NOT corrupt.
        let sidecars = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains(".corrupt-"))
            .count();
        assert_eq!(sidecars, 0, "v1 load must not quarantine");
        // The next flush stamps v2 on disk.
        store.contact_upsert(contact("c2")).unwrap();
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(raw.contains("\"schema_version\": 2"), "flush writes v2: {raw}");
    }

    #[test]
    fn upsert_then_reopen_persists() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("contacts.json");
        let mut store = ContactsStore::open(path.clone());
        store.contact_upsert(contact("c1")).unwrap();
        drop(store);
        let reopened = ContactsStore::open(path);
        assert_eq!(reopened.contacts().len(), 1);
        assert_eq!(reopened.contacts()[0].id, "c1");
        assert_eq!(reopened.contacts()[0].callsign, "W6ABC-7");
    }

    #[test]
    fn upsert_existing_updates_in_place() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("contacts.json");
        let mut store = ContactsStore::open(path.clone());
        let mut c = contact("c1");
        store.contact_upsert(c.clone()).unwrap();
        // Upsert the SAME id with mutated fields + a changed updated_at,
        // preserved created_at.
        c.name = "Pat Renamed".to_string();
        c.updated_at = "2026-06-08T09:30:00+00:00".to_string();
        store.contact_upsert(c.clone()).unwrap();
        assert_eq!(store.contacts().len(), 1, "upsert must not duplicate");
        let stored = &store.contacts()[0];
        assert_eq!(stored.name, "Pat Renamed");
        assert_eq!(stored.created_at, "2026-06-07T12:00:00+00:00");
        assert_eq!(stored.updated_at, "2026-06-08T09:30:00+00:00");
    }

    #[test]
    fn delete_removes() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("contacts.json");
        let mut store = ContactsStore::open(path.clone());
        store.contact_upsert(contact("c1")).unwrap();
        store.contact_delete("c1").unwrap();
        drop(store);
        let reopened = ContactsStore::open(path);
        assert!(reopened.contacts().is_empty());
    }

    #[test]
    fn group_upsert_delete_roundtrip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("contacts.json");
        let mut store = ContactsStore::open(path.clone());
        store.group_upsert(group("g1")).unwrap();
        drop(store);
        let mut reopened = ContactsStore::open(path.clone());
        assert_eq!(reopened.groups().len(), 1);
        let g = &reopened.groups()[0];
        assert_eq!(g.members.len(), 2);
        assert!(matches!(g.members[0], GroupMember::Contact { .. }));
        assert!(matches!(g.members[1], GroupMember::Raw { .. }));
        // delete
        reopened.group_delete("g1").unwrap();
        drop(reopened);
        let again = ContactsStore::open(path);
        assert!(again.groups().is_empty());
    }

    #[test]
    fn unknown_top_level_field_tolerated() {
        // C1: a JSON file carrying an EXTRA top-level key (a future field) must
        // parse fine — known fields preserved, unknown ignored. This proves
        // #[serde(default)] additive tolerance AND that deny_unknown_fields is
        // ABSENT (with it, this parse would fail and trigger quarantine).
        let dir = tempdir().unwrap();
        let path = dir.path().join("contacts.json");
        let json = r#"{
            "schema_version": 1,
            "contacts": [
                {
                    "id": "c1", "name": "Pat", "callsign": "W6ABC-7",
                    "email": null, "tactical": null, "notes": null,
                    "created_at": "2026-06-07T12:00:00+00:00",
                    "updated_at": "2026-06-07T12:00:00+00:00"
                }
            ],
            "groups": [],
            "future_field_from_a_newer_version": {"nested": true}
        }"#;
        std::fs::write(&path, json).unwrap();
        let store = ContactsStore::open(path.clone());
        // Parsed cleanly (NOT quarantined): the contact is present.
        assert_eq!(store.contacts().len(), 1);
        assert_eq!(store.contacts()[0].id, "c1");
        // No corrupt sidecar should have been created.
        let sidecars: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains(".corrupt-"))
            .collect();
        assert!(
            sidecars.is_empty(),
            "tolerated unknown field must NOT trigger quarantine"
        );
    }

    #[test]
    fn open_on_corrupt_file_preserves_original_bytes() {
        // C1: garbage in contacts.json → open() returns empty AND leaves a
        // contacts.json.corrupt-<ts> sidecar holding the ORIGINAL bytes; a
        // subsequent mutate+flush must NOT have destroyed those bytes.
        let dir = tempdir().unwrap();
        let path = dir.path().join("contacts.json");
        let garbage = b"this is not valid json {{{ \x00\x01 broken";
        std::fs::write(&path, garbage).unwrap();

        let mut store = ContactsStore::open(path.clone());
        assert!(
            store.contacts().is_empty(),
            "corrupt file must degrade to empty store"
        );

        // A corrupt sidecar exists and holds the ORIGINAL bytes.
        let sidecar = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .find(|e| e.file_name().to_string_lossy().contains("contacts.json.corrupt-"))
            .expect("expected a contacts.json.corrupt-<ts> sidecar");
        let preserved = std::fs::read(sidecar.path()).unwrap();
        assert_eq!(
            preserved, garbage,
            "corrupt sidecar must hold the original bytes verbatim"
        );

        // A subsequent mutate+flush writes the fresh empty file WITHOUT
        // destroying the preserved bytes.
        store.contact_upsert(contact("c1")).unwrap();
        let preserved_after = std::fs::read(sidecar.path()).unwrap();
        assert_eq!(
            preserved_after, garbage,
            "flush must not clobber the preserved corrupt bytes"
        );
        // And the live file is the new valid one.
        let reopened = ContactsStore::open(path);
        assert_eq!(reopened.contacts().len(), 1);
    }

    #[test]
    fn atomic_write_leaves_no_tmp() {
        // After a flush, no *.tmp remains in the dir (rename consumed it).
        let dir = tempdir().unwrap();
        let path = dir.path().join("contacts.json");
        let mut store = ContactsStore::open(path);
        store.contact_upsert(contact("c1")).unwrap();
        let tmps: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(tmps.is_empty(), "no .tmp file should remain after flush");
    }

    // ------------------------------------------------------------------
    // Observation routing + reachability (spec §AMENDMENT pts. 1/4/8) —
    // the peers-store suite, ported to exact-callsign contact semantics.
    // ------------------------------------------------------------------

    use crate::contacts::observation::{ObservationPhase, ObservedPath, PeerObservation};
    use crate::contacts::reachability::{ChannelBandwidth, ChannelTransport};

    fn now() -> String {
        "2026-07-11T12:00:00-07:00".to_string()
    }

    fn rf_obs(presented: &str, dir: Direction, phase: ObservationPhase) -> PeerObservation {
        PeerObservation {
            path: ObservedPath::Rf {
                transport: ChannelTransport::VaraHf,
                via: vec![],
                freq_hz: Some(7_101_000),
                bandwidth: Some(ChannelBandwidth::Hz { hz: 2300 }),
            },
            direction: dir,
            presented_target: presented.to_string(),
            phase,
        }
    }

    fn telnet_obs(
        presented: &str,
        dir: Direction,
        provenance: Provenance,
        phase: ObservationPhase,
    ) -> PeerObservation {
        PeerObservation {
            path: ObservedPath::Telnet {
                host: "203.0.113.5".into(),
                port: 8772,
                provenance,
            },
            direction: dir,
            presented_target: presented.to_string(),
            phase,
        }
    }

    #[test]
    fn attach_is_exact_callsign_only_no_base_merge() {
        // §AMENDMENT pt. 4: NO base-normalization merging. `W6ABC-7` and
        // `W6ABC` are DISTINCT records; a repeat of the same exact form
        // attaches to its record.
        let dir = tempdir().unwrap();
        let mut s = ContactsStore::open(dir.path().join("contacts.json"));
        let o1 = s
            .apply_observation(&rf_obs("W6ABC-7", Direction::Outgoing, ObservationPhase::B2fOk), now())
            .unwrap();
        assert_eq!(o1.effect, ApplyEffect::CreatedContact);
        let o2 = s
            .apply_observation(&rf_obs("W6ABC", Direction::Incoming, ObservationPhase::Accepted), now())
            .unwrap();
        assert_eq!(o2.effect, ApplyEffect::CreatedContact, "different exact form → new record");
        assert_eq!(s.contacts().len(), 2, "no base merge: two distinct records");
        // A repeat of the exact form ATTACHES (case-insensitively).
        let o3 = s
            .apply_observation(&rf_obs("w6abc-7", Direction::Outgoing, ObservationPhase::B2fOk), now())
            .unwrap();
        assert_eq!(o3.effect, ApplyEffect::UpdatedContact);
        assert_eq!(s.contacts().len(), 2);
        let c7 = s.contacts().iter().find(|c| c.callsign == "W6ABC-7").unwrap();
        assert_eq!(c7.tier, ContactTier::Unconfirmed, "auto-created never lands curated");
        assert_eq!(c7.origin, Origin::Outgoing);
        assert!(c7.name.is_empty(), "auto-created has no fabricated name");
        assert_eq!(c7.channels.len(), 1, "same dedup key → counts, not rows");
        assert_eq!(c7.channels[0].counts.ok, 2);
    }

    #[test]
    fn observation_attaches_to_a_confirmed_contact_without_flipping_tier() {
        // An observation whose callsign exactly equals an existing CONFIRMED
        // contact's callsign attaches to it (any tier) — and never mutates
        // the tier.
        let dir = tempdir().unwrap();
        let mut s = ContactsStore::open(dir.path().join("contacts.json"));
        s.contact_upsert(contact("c1")).unwrap(); // callsign W6ABC-7, Confirmed
        let out = s
            .apply_observation(&rf_obs("W6ABC-7", Direction::Outgoing, ObservationPhase::B2fOk), now())
            .unwrap();
        assert_eq!(out.effect, ApplyEffect::UpdatedContact, "attached, not created");
        assert_eq!(s.contacts().len(), 1);
        let c = &s.contacts()[0];
        assert_eq!(c.tier, ContactTier::Confirmed, "tier untouched by observations");
        assert_eq!(c.name, "Pat Example", "identity fields untouched");
        assert_eq!(c.channels.len(), 1, "reachability accrued on the contact");
    }

    #[test]
    fn channel_key_distinguishes_via_freq_and_bandwidth() {
        let dir = tempdir().unwrap();
        let mut s = ContactsStore::open(dir.path().join("contacts.json"));
        let mut o1 = rf_obs("W6ABC", Direction::Outgoing, ObservationPhase::B2fOk);
        s.apply_observation(&o1, now()).unwrap();
        s.apply_observation(&o1, now()).unwrap(); // same key → counts, not a new row
        assert_eq!(s.contacts()[0].channels.len(), 1);
        assert_eq!(s.contacts()[0].channels[0].counts.ok, 2);
        if let ObservedPath::Rf { ref mut via, .. } = o1.path {
            *via = vec!["DIGI1".into()];
        }
        s.apply_observation(&o1, now()).unwrap(); // different via → distinct channel [R3-6]
        assert_eq!(s.contacts()[0].channels.len(), 2);
    }

    #[test]
    fn rejected_inbound_never_populates_the_roster() {
        let dir = tempdir().unwrap();
        let mut s = ContactsStore::open(dir.path().join("contacts.json"));
        let out = s
            .apply_observation(&rf_obs("EVIL-1", Direction::Incoming, ObservationPhase::Rejected), now())
            .unwrap();
        assert_eq!(out.effect, ApplyEffect::NoRecord);
        assert!(s.contacts().is_empty(), "an attacker knocking is not a contact");
    }

    #[test]
    fn hostile_callsigns_never_reach_the_roster() {
        // [R2-S2][R2-S10]: the write boundary gates on
        // `validate_presented_callsign` — every injection shape below fails it
        // and is dropped as NoRecord before any roster write.
        let dir = tempdir().unwrap();
        let mut s = ContactsStore::open(dir.path().join("contacts.json"));
        for evil in [
            "<img src=x onerror=alert(1)>",
            "W6ABC:extra",
            "A\u{0}B",
            "../../etc/passwd",
            "W6 ABC",
            "`rm -rf`",
        ] {
            let out = s
                .apply_observation(
                    &rf_obs(evil, Direction::Incoming, ObservationPhase::Accepted),
                    now(),
                )
                .unwrap();
            assert_eq!(out.effect, ApplyEffect::NoRecord, "{evil:?} must be dropped");
        }
        assert!(s.contacts().is_empty());
    }

    #[test]
    fn wedged_or_aborted_records_a_fail() {
        let dir = tempdir().unwrap();
        let mut s = ContactsStore::open(dir.path().join("contacts.json"));
        s.apply_observation(
            &rf_obs("W6ABC", Direction::Outgoing, ObservationPhase::AbortedOrWedged),
            now(),
        )
        .unwrap();
        assert_eq!(s.contacts()[0].channels[0].counts.fail, 1);
        assert_eq!(s.contacts()[0].channels[0].counts.ok, 0);
    }

    #[test]
    fn last_ok_is_success_only_on_channels() {
        // T-F Part 0: last_ok is set ONLY by an OK outcome; a FAIL never sets
        // it and never clears a prior success. last_seen bumps on both.
        let dir = tempdir().unwrap();
        let mut s = ContactsStore::open(dir.path().join("contacts.json"));

        // A first FAILED attempt: last_seen set, last_ok still None.
        s.apply_observation(
            &rf_obs("W6ABC", Direction::Outgoing, ObservationPhase::B2fFail),
            "2026-07-11T12:00:00-07:00".into(),
        )
        .unwrap();
        let ch = &s.contacts()[0].channels[0];
        assert_eq!(ch.counts.fail, 1);
        assert_eq!(ch.last_seen, "2026-07-11T12:00:00-07:00");
        assert_eq!(ch.last_ok, None, "a fail must not set last_ok");
        assert_eq!(ch.last_ok_direction, None, "a fail must not set last_ok_direction");

        // A later SUCCESS: last_ok now set to the success instant, and its
        // direction captured atomically with it.
        s.apply_observation(
            &rf_obs("W6ABC", Direction::Outgoing, ObservationPhase::B2fOk),
            "2026-07-11T12:05:00-07:00".into(),
        )
        .unwrap();
        let ch = &s.contacts()[0].channels[0];
        assert_eq!(ch.counts.ok, 1);
        assert_eq!(ch.last_ok.as_deref(), Some("2026-07-11T12:05:00-07:00"));
        assert_eq!(ch.last_ok_direction, Some(Direction::Outgoing));

        // A subsequent FAIL bumps last_seen but PRESERVES the earlier last_ok
        // (and its direction).
        s.apply_observation(
            &rf_obs("W6ABC", Direction::Outgoing, ObservationPhase::AbortedOrWedged),
            "2026-07-11T12:10:00-07:00".into(),
        )
        .unwrap();
        let ch = &s.contacts()[0].channels[0];
        assert_eq!(ch.counts.fail, 1);
        assert_eq!(ch.last_seen, "2026-07-11T12:10:00-07:00", "fail bumps last_seen");
        assert_eq!(
            ch.last_ok.as_deref(),
            Some("2026-07-11T12:05:00-07:00"),
            "a later fail must not clobber the earlier success"
        );
        assert_eq!(
            ch.last_ok_direction,
            Some(Direction::Outgoing),
            "a later fail must not clobber the success direction"
        );
    }

    #[test]
    fn last_ok_direction_survives_an_opposite_direction_failure() {
        // T-F review Finding 1: `direction` mutates on EVERY observation
        // (failures included), so it cannot flavor the success verb. An
        // incoming SUCCESS followed by an outgoing FAILED dial on the SAME
        // channel key must keep last_ok_direction = Incoming — the "heard
        // 3h ago" claim stays literally true even though `direction` now
        // reads Outgoing.
        let dir = tempdir().unwrap();
        let mut s = ContactsStore::open(dir.path().join("contacts.json"));
        s.apply_observation(
            &rf_obs("W6ABC", Direction::Incoming, ObservationPhase::Accepted),
            "2026-07-11T09:00:00-07:00".into(),
        )
        .unwrap();
        s.apply_observation(
            &rf_obs("W6ABC", Direction::Outgoing, ObservationPhase::B2fFail),
            "2026-07-11T12:00:00-07:00".into(),
        )
        .unwrap();
        let ch = &s.contacts()[0].channels[0];
        assert_eq!(ch.direction, Direction::Outgoing, "direction mutated by the failed dial");
        assert_eq!(ch.last_ok.as_deref(), Some("2026-07-11T09:00:00-07:00"));
        assert_eq!(
            ch.last_ok_direction,
            Some(Direction::Incoming),
            "the success's own direction survives the opposite-direction failure"
        );

        // The reverse ordering: outgoing success, then incoming failure
        // (e.g. a later rejected-then-wedged inbound attempt on the same key).
        s.apply_observation(
            &rf_obs("K7XYZ", Direction::Outgoing, ObservationPhase::B2fOk),
            "2026-07-11T09:00:00-07:00".into(),
        )
        .unwrap();
        s.apply_observation(
            &rf_obs("K7XYZ", Direction::Incoming, ObservationPhase::AbortedOrWedged),
            "2026-07-11T12:00:00-07:00".into(),
        )
        .unwrap();
        let ch = &s
            .contacts()
            .iter()
            .find(|c| c.callsign == "K7XYZ")
            .unwrap()
            .channels[0];
        assert_eq!(ch.direction, Direction::Incoming);
        assert_eq!(ch.last_ok_direction, Some(Direction::Outgoing));
    }

    #[test]
    fn last_ok_is_success_only_on_endpoints() {
        // T-F Part 0, endpoint mirror: an operator-dialed telnet endpoint's
        // last_ok tracks successes only.
        let dir = tempdir().unwrap();
        let mut s = ContactsStore::open(dir.path().join("contacts.json"));
        // A failed operator dial: endpoint recorded, last_ok None.
        s.apply_observation(
            &telnet_obs("W6ABC", Direction::Outgoing, Provenance::Operator, ObservationPhase::LoginFailed),
            "2026-07-11T12:00:00-07:00".into(),
        )
        .unwrap();
        assert_eq!(s.contacts()[0].endpoints[0].last_ok, None);
        // A successful operator dial on the SAME endpoint sets last_ok.
        s.apply_observation(
            &telnet_obs("W6ABC", Direction::Outgoing, Provenance::Operator, ObservationPhase::B2fOk),
            "2026-07-11T12:05:00-07:00".into(),
        )
        .unwrap();
        assert_eq!(
            s.contacts()[0].endpoints[0].last_ok.as_deref(),
            Some("2026-07-11T12:05:00-07:00")
        );
    }

    #[test]
    fn slash_p_presented_form_is_stored() {
        // [R3-F2]: the write boundary gates on validate_presented_callsign, so
        // a legit portable form survives rather than being dropped.
        let dir = tempdir().unwrap();
        let mut s = ContactsStore::open(dir.path().join("contacts.json"));
        let out = s
            .apply_observation(&rf_obs("W6ABC/P", Direction::Outgoing, ObservationPhase::B2fOk), now())
            .unwrap();
        assert_eq!(out.effect, ApplyEffect::CreatedContact);
        assert_eq!(s.contacts()[0].callsign, "W6ABC/P");
    }

    #[test]
    fn inbound_observation_never_writes_an_operator_endpoint() {
        // Monotonic provenance [R4-4][R2-S8]: an inbound observation claiming
        // Operator provenance is downgraded to ObservedIncoming at the write
        // boundary; an OUTBOUND operator dial legitimately records Operator
        // (how a password-bearing endpoint is born).
        let dir = tempdir().unwrap();
        let mut s = ContactsStore::open(dir.path().join("contacts.json"));
        s.apply_observation(
            &telnet_obs("W6ABC", Direction::Incoming, Provenance::Operator, ObservationPhase::Accepted),
            now(),
        )
        .unwrap();
        assert_eq!(
            s.contacts()[0].endpoints[0].provenance,
            Provenance::ObservedIncoming,
            "inbound can never mint Operator"
        );
        s.apply_observation(
            &telnet_obs("K7XYZ", Direction::Outgoing, Provenance::Operator, ObservationPhase::B2fOk),
            now(),
        )
        .unwrap();
        let dialed = s.contacts().iter().find(|c| c.callsign == "K7XYZ").unwrap();
        assert_eq!(dialed.endpoints[0].provenance, Provenance::Operator);
        // A later inbound observation of the same host:port must NOT touch the
        // Operator endpoint (distinct provenance in the dedup key).
        s.apply_observation(
            &telnet_obs("K7XYZ", Direction::Incoming, Provenance::ObservedIncoming, ObservationPhase::Accepted),
            now(),
        )
        .unwrap();
        let dialed = s.contacts().iter().find(|c| c.callsign == "K7XYZ").unwrap();
        assert_eq!(
            dialed.endpoints.iter().filter(|e| e.provenance == Provenance::Operator).count(),
            1
        );
        assert_eq!(dialed.endpoints.len(), 2, "observed row is a distinct endpoint");
    }

    #[test]
    fn per_contact_channel_cap_holds() {
        // R3-F4: rotated observations (distinct via) on ONE contact cannot
        // grow it without bound — the per-contact channel cap holds.
        let dir = tempdir().unwrap();
        let mut s = ContactsStore::open(dir.path().join("contacts.json"));
        for i in 0..80u32 {
            let mut o = rf_obs("W6ABC", Direction::Outgoing, ObservationPhase::B2fOk);
            if let ObservedPath::Rf { ref mut via, .. } = o.path {
                *via = vec![format!("DIGI{i}")];
            }
            let ts = format!("2026-07-11T12:{:02}:{:02}-07:00", i / 60, i % 60);
            s.apply_observation(&o, ts).unwrap();
        }
        assert_eq!(s.contacts().len(), 1, "all observations on one contact");
        assert_eq!(s.contacts()[0].channels.len(), PER_CONTACT_CHANNEL_CAP);
    }

    #[test]
    fn lru_eviction_evicts_stalest_unconfirmed_only_and_reports_secrets() {
        // §AMENDMENT pt. 8: the auto cap guards Unconfirmed ONLY — Confirmed
        // is never evicted regardless of age — and eviction reports the
        // victims' (contact_id, endpoint_id) pairs for the keyring cascade.
        // Exercises the private eviction helper directly with a small cap
        // (the production AUTO_CONTACT_CAP=1000 path calls the same helper
        // from apply_observation on every create).
        let dir = tempdir().unwrap();
        let mut s = ContactsStore::open(dir.path().join("contacts.json"));
        // A Confirmed contact OLDER than everything else.
        let mut old_confirmed = contact("c-conf");
        old_confirmed.created_at = "2026-01-01T00:00:00-07:00".to_string();
        s.contact_upsert(old_confirmed).unwrap();
        // Four unconfirmed records with ascending last-seen; the second one
        // carries a telnet endpoint (its secret pair must be reported).
        for (i, call) in ["K1AAA", "K2BBB", "K3CCC", "K4DDD"].iter().enumerate() {
            let ts = format!("2026-07-11T12:00:0{i}-07:00");
            if *call == "K2BBB" {
                s.apply_observation(
                    &telnet_obs(call, Direction::Incoming, Provenance::ObservedIncoming, ObservationPhase::Accepted),
                    ts,
                )
                .unwrap();
            } else {
                s.apply_observation(&rf_obs(call, Direction::Incoming, ObservationPhase::Accepted), ts)
                    .unwrap();
            }
        }
        assert_eq!(s.contacts().len(), 5);
        let k2_pair = {
            let k2 = s.contacts().iter().find(|c| c.callsign == "K2BBB").unwrap();
            (k2.id.clone(), k2.endpoints[0].id.clone())
        };

        let evicted = s.evict_unconfirmed_over(2);
        // K1AAA (oldest, no endpoints) and K2BBB (next-oldest, one endpoint)
        // are evicted; K3CCC/K4DDD stay; the Confirmed record — the OLDEST in
        // the whole file — is untouched.
        assert_eq!(s.contacts().len(), 3);
        assert!(s.contacts().iter().any(|c| c.id == "c-conf"), "Confirmed never evicted");
        assert!(!s.contacts().iter().any(|c| c.callsign == "K1AAA"));
        assert!(!s.contacts().iter().any(|c| c.callsign == "K2BBB"));
        assert_eq!(evicted, vec![k2_pair], "endpoint secret pairs reported for cascade");
    }

    #[test]
    fn contact_delete_returns_endpoint_ids_for_the_keyring_cascade() {
        let dir = tempdir().unwrap();
        let mut s = ContactsStore::open(dir.path().join("contacts.json"));
        s.apply_observation(
            &telnet_obs("W6ABC", Direction::Outgoing, Provenance::Operator, ObservationPhase::B2fOk),
            now(),
        )
        .unwrap();
        let (cid, eid) = {
            let c = &s.contacts()[0];
            (c.id.clone(), c.endpoints[0].id.clone())
        };
        let ids = s.contact_delete(&cid).unwrap();
        assert_eq!(ids, vec![eid], "delete reports its endpoints for the cascade");
        assert!(s.contacts().is_empty());
        // Absent id → no-op, empty cascade.
        assert!(s.contact_delete("ghost").unwrap().is_empty());
    }

    #[test]
    fn contact_confirm_flips_tier_and_errors_on_unknown_id() {
        let dir = tempdir().unwrap();
        let mut s = ContactsStore::open(dir.path().join("contacts.json"));
        s.apply_observation(&rf_obs("W6ABC-7", Direction::Incoming, ObservationPhase::Accepted), now())
            .unwrap();
        let id = s.contacts()[0].id.clone();
        assert_eq!(s.contacts()[0].tier, ContactTier::Unconfirmed);
        s.contact_confirm(&id, "2026-07-11T13:00:00-07:00".into()).unwrap();
        assert_eq!(s.contacts()[0].tier, ContactTier::Confirmed);
        assert_eq!(s.contacts()[0].updated_at, "2026-07-11T13:00:00-07:00");
        // Idempotent on an already-confirmed record.
        s.contact_confirm(&id, "2026-07-11T13:00:01-07:00".into()).unwrap();
        assert_eq!(s.contacts()[0].tier, ContactTier::Confirmed);
        // Unknown id → Validation error.
        assert!(matches!(
            s.contact_confirm("ghost", now()),
            Err(ContactsError::Validation(_))
        ));
    }

    #[test]
    fn observation_write_round_trips_through_reopen() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("contacts.json");
        let mut s = ContactsStore::open(path.clone());
        s.apply_observation(&rf_obs("W6ABC-7", Direction::Outgoing, ObservationPhase::B2fOk), now())
            .unwrap();
        let reopened = ContactsStore::open(path);
        assert_eq!(reopened.contacts().len(), 1);
        assert_eq!(reopened.contacts()[0].callsign, "W6ABC-7");
        assert_eq!(reopened.contacts()[0].tier, ContactTier::Unconfirmed);
        assert_eq!(reopened.contacts()[0].channels.len(), 1);
    }

    // ------------------------------------------------------------------
    // unambiguous_operator_endpoint — the conservative legacy-keyring
    // migration resolver [R5-5], re-targeted from peers to contacts.
    // ------------------------------------------------------------------

    fn contact_with_endpoints(id: &str, callsign: &str, endpoints: Vec<Endpoint>) -> Contact {
        let mut c = contact(id);
        c.callsign = callsign.to_string();
        c.endpoints = endpoints;
        c
    }

    fn ep(id: &str, provenance: Provenance) -> Endpoint {
        Endpoint {
            id: id.to_string(),
            host: "peer.example.org".to_string(),
            port: 8772,
            provenance,
            last_seen: "2026-07-11T12:00:00-07:00".to_string(),
            last_ok: None,
        }
    }

    #[test]
    fn unambiguous_resolver_finds_the_single_operator_endpoint() {
        let mut file = ContactsFile::default();
        file.contacts.push(contact_with_endpoints(
            "c1",
            "W6ABC",
            vec![ep("e-op", Provenance::Operator), ep("e-obs", Provenance::ObservedIncoming)],
        ));
        assert_eq!(
            unambiguous_operator_endpoint(&file, "w6abc"),
            Some(("c1".to_string(), "e-op".to_string())),
            "one contact + one Operator endpoint → unambiguous (case-insensitive)"
        );
    }

    #[test]
    fn ambiguous_resolver_cases_return_none() {
        // Multiple contacts on the callsign → ambiguous.
        let mut two = ContactsFile::default();
        two.contacts.push(contact_with_endpoints("c1", "W6ABC", vec![ep("e1", Provenance::Operator)]));
        two.contacts.push(contact_with_endpoints("c2", "W6ABC", vec![]));
        assert_eq!(unambiguous_operator_endpoint(&two, "W6ABC"), None);

        // Zero Operator endpoints (observed-only) → ambiguous.
        let mut observed = ContactsFile::default();
        observed
            .contacts
            .push(contact_with_endpoints("c1", "W6ABC", vec![ep("e1", Provenance::ObservedIncoming)]));
        assert_eq!(unambiguous_operator_endpoint(&observed, "W6ABC"), None);

        // Multiple Operator endpoints → ambiguous.
        let mut multi = ContactsFile::default();
        multi.contacts.push(contact_with_endpoints(
            "c1",
            "W6ABC",
            vec![ep("e1", Provenance::Operator), ep("e2", Provenance::Operator)],
        ));
        assert_eq!(unambiguous_operator_endpoint(&multi, "W6ABC"), None);

        // No matching contact at all, and the exact-match rule: an SSID'd
        // record does NOT answer for the base form.
        let mut ssid = ContactsFile::default();
        ssid.contacts
            .push(contact_with_endpoints("c1", "W6ABC-7", vec![ep("e1", Provenance::Operator)]));
        assert_eq!(unambiguous_operator_endpoint(&ssid, "W6ABC"), None);
        assert_eq!(unambiguous_operator_endpoint(&ssid, ""), None);
    }
}
