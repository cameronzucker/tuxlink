//! Peer JSON store — the `peers.json` roster backing the first-class peer
//! model (spec §2/§3: docs/superpowers/specs/2026-07-10-p2p-peer-model-design.md).
//!
//! Mirrors the `contacts/store.rs` house pattern: INFALLIBLE
//! [`PeersStore::open`] (corrupt → timestamped `.corrupt-*` sidecar + empty
//! store, never blocks startup, never overwrites the corrupt bytes in place),
//! atomic `flush` (`serialize → <name>.tmp → rename`), `#[serde(default)]`
//! additive tolerance, hand-written `Default` on [`crate::peers::model::PeersFile`].
//!
//! Store-specific invariants (all pinned by the inline tests):
//! - Dedup anchor is `canonical_base`, except `IdentityKind::Tactical` records
//!   anchor on their full presented string [R4-6].
//! - Once any record on a base carries `do_not_merge` (an operator split),
//!   routing on that base is by EXACT presented callsign; a non-matching
//!   observation is held as a `conflict: true` record, never silently applied
//!   to the wrong twin [R5-4].
//! - Endpoint provenance is monotonic [R4-4][R2-S8]: an INBOUND observation
//!   may never create or mutate an `Operator` endpoint (it is recorded as
//!   `ObservedIncoming`); an OUTBOUND operator dial legitimately records an
//!   `Operator` endpoint (that is how a telnet favorite is born, Task 16).
//!   Only [`PeersStore::promote_endpoint`] sets `Operator` in place [R5-5].
//! - Bounding (R3-F4): auto records are LRU-capped
//!   ([`crate::peers::model::AUTO_PEER_CAP`]); `conflict` records have a
//!   SEPARATE [`CONFLICT_CAP`] so conflict spam only ever evicts conflict spam,
//!   never a real peer; and each peer's `channels` / `presented_callsigns` are
//!   per-peer capped so rotated observations on one peer cannot grow it without
//!   bound. Conflict-record CREATION is distinguishable via
//!   [`PeersStore::would_create_conflict_record`] so Task 9's inbound limiter
//!   can gate it (a split base always "exists", so the base-exists limiter skip
//!   would otherwise let conflict creation bypass the limiter entirely).
//! - The presented-callsign WRITE boundary gates on
//!   [`crate::winlink::callsign::validate_presented_callsign`] (NOT
//!   `sanitize_display`), so a legitimate portable form like `W6ABC/P` is
//!   stored rather than dropped [R3-F2].

use crate::peers::model::{
    AttemptCounts, Channel, Direction, Endpoint, IdentityKind, Origin, Peer, PeersFile,
    Provenance, RecordSource, AUTO_PEER_CAP,
};
use serde::Serialize;
use std::path::PathBuf;
use thiserror::Error;

/// Max retained `conflict: true` records. A held-for-manual-association record
/// is created for every unmatched observation on a split base; without a
/// SEPARATE cap, folding conflicts into the auto-LRU would let fresh conflict
/// spam evict the OLDEST legit auto peers (conflicts are always the newest).
/// Oldest-among-conflicts is evicted so conflict spam only evicts conflicts.
const CONFLICT_CAP: usize = 100;

/// Max `channels` per peer. Rotated observations (via/freq churn) on ONE peer
/// must not grow it without bound; oldest-`last_seen` is evicted.
const PER_PEER_CHANNEL_CAP: usize = 64;

/// Max `presented_callsigns` per peer; oldest-recorded (front) is evicted.
const PER_PEER_PRESENTED_CAP: usize = 64;

/// Serializable error projection for the IPC boundary — mirrors the
/// `{ kind, detail }` discriminated-union shape used across the app.
#[derive(Debug, Error, Serialize)]
#[serde(tag = "kind", content = "detail")]
pub enum PeersError {
    #[error("io: {0}")]
    Io(String),
    #[error("serde: {0}")]
    Serde(String),
    #[error("validation: {0}")]
    Validation(String),
}

/// The roster effect of an [`PeersStore::apply_observation`] call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplyEffect {
    CreatedPeer,
    UpdatedPeer,
    /// An observation on a split base matched no twin's presented callsign; a
    /// `conflict: true` record was created for manual association [R5-4].
    ConflictHeld,
    /// A rejected/unauthorized inbound observation — no roster write.
    NoRecord,
}

/// Per-endpoint disposition reported by [`PeersStore::merge`] so the command
/// layer can run the keyring cascade [R2-S7] without re-deriving the dedup
/// decision. Endpoint ids are stable and never reminted by a merge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AbsorbedEndpoint {
    /// The absorbed endpoint moved onto the kept record with its ORIGINAL id.
    /// Its keyring secret must be re-keyed `(absorb_id, id)` → `(keep_id, id)`
    /// or future lookups under the kept peer will miss it.
    Survived { endpoint_id: String },
    /// The absorbed endpoint matched an existing keep endpoint by
    /// `(host, port, provenance)` and was dropped. Its id exists nowhere in
    /// the roster anymore, so its keyring secret has no valid target and must
    /// be deleted.
    Deduped { endpoint_id: String },
}

/// The peers store: an in-memory [`PeersFile`] plus the path it persists to.
/// Mutations flush eagerly. Construct via [`PeersStore::open`].
pub struct PeersStore {
    path: PathBuf,
    file: PeersFile,
}

impl PeersStore {
    /// Open the store at `path`. INFALLIBLE — always returns a usable store.
    ///
    /// - Missing file → default empty store.
    /// - Present + parseable → the parsed file.
    /// - Present + UNparseable: rename the file to `<name>.corrupt-<utc-ts>`
    ///   (preserving the original bytes), `eprintln!` a warning, then return
    ///   the default empty store. A later flush writes only to `path`, leaving
    ///   the sidecar intact.
    pub fn open(path: PathBuf) -> Self {
        let file = match std::fs::read(&path) {
            Ok(bytes) => match serde_json::from_slice::<PeersFile>(&bytes) {
                Ok(parsed) => parsed,
                Err(e) => {
                    Self::quarantine_corrupt(&path, &bytes);
                    eprintln!(
                        "peers: {} is unparseable, starting empty (original preserved): {e}",
                        path.display()
                    );
                    PeersFile::default()
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => PeersFile::default(),
            Err(e) => {
                if let Ok(bytes) = std::fs::read(&path) {
                    Self::quarantine_corrupt(&path, &bytes);
                }
                eprintln!("peers: failed to read {}, starting empty: {e}", path.display());
                PeersFile::default()
            }
        };
        Self { path, file }
    }

    /// Rename the unreadable file to a timestamped `.corrupt-*` sidecar,
    /// preserving the original bytes; fall back to a copy-write if the rename
    /// fails (best-effort preservation; never panics).
    fn quarantine_corrupt(path: &std::path::Path, original: &[u8]) {
        let ts = chrono::Utc::now().format("%Y%m%dT%H%M%SZ");
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "peers.json".to_string());
        let corrupt = path.with_file_name(format!("{name}.corrupt-{ts}"));
        if let Err(e) = std::fs::rename(path, &corrupt) {
            eprintln!(
                "peers: could not rename corrupt {} → {} ({e}); copying bytes instead",
                path.display(),
                corrupt.display()
            );
            let _ = std::fs::write(&corrupt, original);
        }
    }

    /// Persist the in-memory file atomically: serialize → write to a sibling
    /// `<name>.tmp` → `rename` over the final path. `create_dir_all(parent)`
    /// first. Uses `format!("{}.tmp", name)` so the suffix is `peers.json.tmp`.
    fn flush(&self) -> Result<(), PeersError> {
        let json = serde_json::to_string_pretty(&self.file)
            .map_err(|e| PeersError::Serde(e.to_string()))?;
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| PeersError::Io(e.to_string()))?;
        }
        let name = self
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "peers.json".to_string());
        let tmp = self.path.with_file_name(format!("{name}.tmp"));
        std::fs::write(&tmp, json).map_err(|e| PeersError::Io(e.to_string()))?;
        std::fs::rename(&tmp, &self.path).map_err(|e| PeersError::Io(e.to_string()))?;
        Ok(())
    }

    /// The whole in-memory file (read-only view).
    pub fn file(&self) -> &PeersFile {
        &self.file
    }

    /// Route an observation to its peer record and apply it (spec §2/§3).
    ///
    /// The caller (Task 11's recorder) has already classified rejected-inbound
    /// via `recorder::classify`; the `Rejected`/`NoRecord` re-check here is
    /// defense-in-depth — a `NoRecord` phase is never a roster write.
    pub fn apply_observation(
        &mut self,
        obs: &crate::peers::recorder::PeerObservation,
        now: String,
    ) -> Result<ApplyEffect, PeersError> {
        use crate::peers::recorder::{classify, Classified, ObservedPath};
        let bucket = classify(obs.phase);
        if matches!(bucket, Classified::NoRecord) {
            return Ok(ApplyEffect::NoRecord);
        }
        let presented = obs.presented_target.trim().to_ascii_uppercase();
        // Write-boundary floor [R3-F2]: gate on the PRESENTED validator, not
        // `sanitize_display`, so a legit portable form (`W6ABC/P`) is stored.
        if crate::winlink::callsign::validate_presented_callsign(&presented).is_err() {
            return Ok(ApplyEffect::NoRecord);
        }
        let base = crate::winlink::callsign::canonical_base(&presented);

        // ── Routing [R5-4]: split bases route by exact presented form ────────
        let base_has_split = self
            .file
            .peers
            .iter()
            .any(|p| p.canonical_base == base && p.do_not_merge);
        let idx = if base_has_split {
            match self.file.peers.iter().position(|p| {
                p.canonical_base == base && p.presented_callsigns.iter().any(|c| c == &presented)
            }) {
                Some(i) => Some(i),
                None => {
                    // Unmatched form on a split base: hold for manual
                    // association — never silently update the wrong twin. This
                    // is the ONLY conflict-creation path; it is gated by the
                    // inbound limiter via `would_create_conflict_record` (Task
                    // 9) and separately bounded by `CONFLICT_CAP`.
                    let mut held = self.new_auto_peer(&base, &presented, obs, &now);
                    held.conflict = true;
                    self.file.peers.push(held);
                    self.evict_conflicts_over_cap();
                    self.flush()?;
                    return Ok(ApplyEffect::ConflictHeld);
                }
            }
        } else {
            self.file.peers.iter().position(|p| {
                if p.identity_kind == IdentityKind::Tactical {
                    // Tactical anchors on the full presented string [R4-6].
                    p.presented_callsigns.iter().any(|c| c == &presented)
                } else {
                    p.canonical_base == base
                }
            })
        };

        let created = idx.is_none();
        let idx = match idx {
            Some(i) => i,
            None => {
                let p = self.new_auto_peer(&base, &presented, obs, &now);
                self.file.peers.push(p);
                self.evict_over_cap();
                self.file.peers.len() - 1
            }
        };

        // ── Apply the observation to the record ──────────────────────────────
        let ok = matches!(bucket, Classified::Ok);
        {
            let p = &mut self.file.peers[idx];
            if !p.presented_callsigns.iter().any(|c| c == &presented) {
                p.presented_callsigns.push(presented.clone());
            }
            if ok {
                p.last_connected_at = Some(now.clone());
            }
            match &obs.path {
                ObservedPath::Rf {
                    transport,
                    via,
                    freq_hz,
                    bandwidth,
                } => {
                    let key_match = |c: &Channel| {
                        c.transport == *transport
                            && c.target_callsign == presented
                            && c.via == *via
                            && c.freq_hz == *freq_hz
                            && c.bandwidth == *bandwidth
                    };
                    if let Some(ch) = p.channels.iter_mut().find(|c| key_match(c)) {
                        if ok {
                            ch.counts.ok = ch.counts.ok.saturating_add(1);
                        } else {
                            ch.counts.fail = ch.counts.fail.saturating_add(1);
                        }
                        ch.direction = obs.direction;
                        ch.last_seen = now.clone();
                    } else {
                        p.channels.push(Channel {
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
                        });
                    }
                }
                ObservedPath::Telnet {
                    host,
                    port,
                    provenance,
                } => {
                    // Monotonic provenance [R4-4]: an INBOUND observation NEVER
                    // creates or mutates an `Operator` endpoint (amendment (a):
                    // the downgrade is conditioned on direction == Incoming — an
                    // outbound operator dial legitimately records `Operator`).
                    let prov = if *provenance == Provenance::Operator
                        && obs.direction == Direction::Incoming
                    {
                        Provenance::ObservedIncoming
                    } else {
                        *provenance
                    };
                    let hostn = host.trim().to_ascii_lowercase();
                    if let Some(ep) = p
                        .endpoints
                        .iter_mut()
                        .find(|e| e.host == hostn && e.port == *port && e.provenance == prov)
                    {
                        ep.last_seen = now.clone();
                    } else {
                        p.endpoints.push(Endpoint {
                            id: uuid::Uuid::new_v4().to_string(),
                            host: hostn,
                            port: *port,
                            provenance: prov,
                            last_seen: now.clone(),
                        });
                    }
                }
            }
            Self::enforce_per_peer_caps(p);
        }
        self.flush()?;
        Ok(if created {
            ApplyEffect::CreatedPeer
        } else {
            ApplyEffect::UpdatedPeer
        })
    }

    /// Would this observation CREATE a new `conflict: true` record? Pure /
    /// read-only seam for Task 9's inbound limiter: a split base always
    /// "exists", so the recorder's base-exists limiter skip would otherwise let
    /// conflict creation bypass the limiter entirely. Task 9 calls this before
    /// `apply_observation` and runs the failed-path limiter when it returns
    /// true. Mirrors the routing decision in `apply_observation` exactly.
    pub fn would_create_conflict_record(
        &self,
        obs: &crate::peers::recorder::PeerObservation,
    ) -> bool {
        use crate::peers::recorder::{classify, Classified};
        if matches!(classify(obs.phase), Classified::NoRecord) {
            return false;
        }
        let presented = obs.presented_target.trim().to_ascii_uppercase();
        if crate::winlink::callsign::validate_presented_callsign(&presented).is_err() {
            return false;
        }
        let base = crate::winlink::callsign::canonical_base(&presented);
        let base_has_split = self
            .file
            .peers
            .iter()
            .any(|p| p.canonical_base == base && p.do_not_merge);
        if !base_has_split {
            return false;
        }
        // A conflict is created iff no record on this base carries the form.
        !self.file.peers.iter().any(|p| {
            p.canonical_base == base && p.presented_callsigns.iter().any(|c| c == &presented)
        })
    }

    fn new_auto_peer(
        &self,
        base: &str,
        presented: &str,
        obs: &crate::peers::recorder::PeerObservation,
        now: &str,
    ) -> Peer {
        Peer {
            id: uuid::Uuid::new_v4().to_string(),
            canonical_base: base.to_string(),
            presented_callsigns: vec![presented.to_string()],
            identity_kind: IdentityKind::Unknown,
            do_not_merge: false,
            conflict: false,
            source: RecordSource::Auto,
            origin: match obs.direction {
                Direction::Incoming => Origin::Incoming,
                Direction::Outgoing => Origin::Outgoing,
                Direction::Unknown => Origin::Unknown,
            },
            contact_id: None,
            grid: None,
            note: String::new(),
            created_at: now.to_string(),
            last_connected_at: None,
            channels: vec![],
            endpoints: vec![],
        }
    }

    /// LRU eviction among non-conflict `Auto` records only [R2-S6].
    /// `Manual`/`OperatorPinned` and `conflict` records are never touched here
    /// (conflicts are bounded separately by [`Self::evict_conflicts_over_cap`]).
    fn evict_over_cap(&mut self) {
        loop {
            let auto: Vec<usize> = self
                .file
                .peers
                .iter()
                .enumerate()
                .filter(|(_, p)| p.source == RecordSource::Auto && !p.conflict)
                .map(|(i, _)| i)
                .collect();
            if auto.len() <= AUTO_PEER_CAP {
                return;
            }
            // Oldest activity = last_connected_at, falling back to created_at.
            let lru = auto
                .into_iter()
                .min_by(|&a, &b| {
                    let ka = self.file.peers[a]
                        .last_connected_at
                        .as_deref()
                        .unwrap_or(&self.file.peers[a].created_at);
                    let kb = self.file.peers[b]
                        .last_connected_at
                        .as_deref()
                        .unwrap_or(&self.file.peers[b].created_at);
                    ka.cmp(kb)
                })
                .expect("non-empty by the cap check");
            self.file.peers.remove(lru);
        }
    }

    /// Bound `conflict` records at [`CONFLICT_CAP`], evicting the oldest
    /// (by `created_at`) among conflicts only — so conflict spam evicts
    /// conflict spam, never a real peer (R3-F4).
    fn evict_conflicts_over_cap(&mut self) {
        loop {
            let conflicts: Vec<usize> = self
                .file
                .peers
                .iter()
                .enumerate()
                .filter(|(_, p)| p.conflict)
                .map(|(i, _)| i)
                .collect();
            if conflicts.len() <= CONFLICT_CAP {
                return;
            }
            let oldest = conflicts
                .into_iter()
                .min_by(|&a, &b| self.file.peers[a].created_at.cmp(&self.file.peers[b].created_at))
                .expect("non-empty by the cap check");
            self.file.peers.remove(oldest);
        }
    }

    /// Per-peer bounding (R3-F4): cap `channels` (oldest-`last_seen` evicted)
    /// and `presented_callsigns` (oldest-recorded/front evicted) so rotated
    /// observations on one peer cannot grow it without bound.
    fn enforce_per_peer_caps(p: &mut Peer) {
        while p.channels.len() > PER_PEER_CHANNEL_CAP {
            let victim = p
                .channels
                .iter()
                .enumerate()
                .min_by(|(_, a), (_, b)| a.last_seen.cmp(&b.last_seen))
                .map(|(i, _)| i);
            match victim {
                Some(i) => {
                    p.channels.remove(i);
                }
                None => break,
            }
        }
        while p.presented_callsigns.len() > PER_PEER_PRESENTED_CAP {
            p.presented_callsigns.remove(0);
        }
    }

    /// Insert an operator-added peer, or replace the existing one with the same
    /// `id`. Validates `canonical_base` + each `presented_callsigns` entry through
    /// [`crate::winlink::callsign::validate_presented_callsign`] (NOT
    /// `sanitize_display`) [R3-F2], matching [`Self::apply_observation`]'s write
    /// boundary and this module's documented presented-callsign write-boundary
    /// invariant — so a legitimate portable form like `W6ABC/P` entered by hand is
    /// stored rather than dropped, exactly as an observed one would be.
    /// `sanitize_display` remains the floor for free-text display fields (notes),
    /// which this manual path does not gate. Flushes on success.
    pub fn upsert_manual(&mut self, peer: Peer) -> Result<(), PeersError> {
        if let Err(e) = crate::winlink::callsign::validate_presented_callsign(&peer.canonical_base) {
            return Err(PeersError::Validation(format!(
                "invalid canonical_base {:?}: {e}",
                peer.canonical_base
            )));
        }
        for pc in &peer.presented_callsigns {
            if let Err(e) = crate::winlink::callsign::validate_presented_callsign(pc) {
                return Err(PeersError::Validation(format!(
                    "invalid presented callsign {pc:?}: {e}"
                )));
            }
        }
        match self.file.peers.iter_mut().find(|p| p.id == peer.id) {
            Some(existing) => *existing = peer,
            None => self.file.peers.push(peer),
        }
        self.flush()
    }

    /// Remove a peer by id (no-op if absent). Returns the removed peer's
    /// endpoint ids for the keyring-secret cascade the `peer_delete` command
    /// (Task 11) runs via the Task 10 `p2p_endpoint_password_delete` API.
    /// Flushes when a record was actually removed.
    pub fn delete_peer(&mut self, id: &str) -> Result<Vec<String>, PeersError> {
        match self.file.peers.iter().position(|p| p.id == id) {
            Some(idx) => {
                let removed = self.file.peers.remove(idx);
                let endpoint_ids = removed.endpoints.iter().map(|e| e.id.clone()).collect();
                self.flush()?;
                Ok(endpoint_ids)
            }
            None => Ok(vec![]),
        }
    }

    /// Merge `absorb_id` into `keep_id`: move the absorbed record's
    /// presented forms / channels / endpoints onto the kept record (dedup by
    /// their keys), delete the absorbed record, and return each absorbed
    /// endpoint's [`AbsorbedEndpoint`] disposition for the keyring cascade.
    ///
    /// The `peer_merge` COMMAND (Task 11) performs that cascade: a `Survived`
    /// endpoint keeps its id but now lives under `keep_id`, so its secret is
    /// re-keyed `(absorb_id, eid)` → `(keep_id, eid)`; a `Deduped` endpoint no
    /// longer exists anywhere, so its secret is deleted. (An earlier revision
    /// attributed the re-key to Task 10 — wrong: Task 10 shipped only the
    /// legacy-CALLSIGN → id-keyed migration, not a merge re-key primitive.)
    pub fn merge(
        &mut self,
        keep_id: &str,
        absorb_id: &str,
    ) -> Result<Vec<AbsorbedEndpoint>, PeersError> {
        let keep_idx = self
            .file
            .peers
            .iter()
            .position(|p| p.id == keep_id)
            .ok_or_else(|| PeersError::Validation(format!("keep peer {keep_id:?} not found")))?;
        let absorb_idx = self
            .file
            .peers
            .iter()
            .position(|p| p.id == absorb_id)
            .ok_or_else(|| PeersError::Validation(format!("absorb peer {absorb_id:?} not found")))?;
        if keep_idx == absorb_idx {
            return Err(PeersError::Validation("cannot merge a peer into itself".into()));
        }

        let absorbed = self.file.peers.remove(absorb_idx);
        // The remove shifted indices; re-resolve the keep record by id.
        let keep_idx = self
            .file
            .peers
            .iter()
            .position(|p| p.id == keep_id)
            .expect("keep record still present after removing the absorbed one");
        let keep = &mut self.file.peers[keep_idx];

        for pc in absorbed.presented_callsigns {
            if !keep.presented_callsigns.contains(&pc) {
                keep.presented_callsigns.push(pc);
            }
        }
        for ch in absorbed.channels {
            if let Some(existing) = keep.channels.iter_mut().find(|c| channel_key_eq(c, &ch)) {
                existing.counts.ok = existing.counts.ok.saturating_add(ch.counts.ok);
                existing.counts.fail = existing.counts.fail.saturating_add(ch.counts.fail);
                if ch.last_seen > existing.last_seen {
                    existing.last_seen = ch.last_seen;
                    existing.direction = ch.direction;
                }
            } else {
                keep.channels.push(ch);
            }
        }
        let mut dispositions: Vec<AbsorbedEndpoint> = Vec::with_capacity(absorbed.endpoints.len());
        for ep in absorbed.endpoints {
            if let Some(existing) = keep
                .endpoints
                .iter_mut()
                .find(|e| e.host == ep.host && e.port == ep.port && e.provenance == ep.provenance)
            {
                if ep.last_seen > existing.last_seen {
                    existing.last_seen = ep.last_seen;
                }
                // Matched an existing keep endpoint by (host, port, provenance):
                // the absorbed row is dropped, so its id no longer exists
                // anywhere — its keyring secret must be deleted, not re-keyed.
                dispositions.push(AbsorbedEndpoint::Deduped { endpoint_id: ep.id });
            } else {
                // Moved onto the kept record WITH ITS ORIGINAL id — the keyring
                // secret must follow it from (absorb_id, id) to (keep_id, id).
                dispositions.push(AbsorbedEndpoint::Survived {
                    endpoint_id: ep.id.clone(),
                });
                keep.endpoints.push(ep);
            }
        }
        Self::enforce_per_peer_caps(keep);
        self.flush()?;
        Ok(dispositions)
    }

    /// Split the named presented forms (and their exact-matching channels) off
    /// `peer_id` into a fresh clone, set `do_not_merge` on BOTH the source and
    /// the clone (base-anchored auto-merge is off for that base forever [R5-4]),
    /// and return the new clone's id.
    pub fn split(
        &mut self,
        peer_id: &str,
        moved_presented: Vec<String>,
        now: String,
    ) -> Result<String, PeersError> {
        let idx = self
            .file
            .peers
            .iter()
            .position(|p| p.id == peer_id)
            .ok_or_else(|| PeersError::Validation(format!("peer {peer_id:?} not found")))?;
        // Normalize to the stored (uppercased) presented representation.
        let moved: Vec<String> = moved_presented
            .iter()
            .map(|s| s.trim().to_ascii_uppercase())
            .collect();
        let new_id = uuid::Uuid::new_v4().to_string();

        let clone = {
            let src = &self.file.peers[idx];
            let mut clone = src.clone();
            clone.id = new_id.clone();
            clone.do_not_merge = true;
            clone.conflict = false;
            clone.created_at = now.clone();
            clone.presented_callsigns = src
                .presented_callsigns
                .iter()
                .filter(|c| moved.iter().any(|m| m == *c))
                .cloned()
                .collect();
            clone.channels = src
                .channels
                .iter()
                .filter(|ch| moved.contains(&ch.target_callsign))
                .cloned()
                .collect();
            // Endpoints (telnet host:port) are not tied to a presented form;
            // they stay with the source record.
            clone.endpoints = vec![];
            clone
        };

        {
            let src = &mut self.file.peers[idx];
            src.presented_callsigns
                .retain(|c| !moved.iter().any(|m| m == c));
            src.channels
                .retain(|ch| !moved.contains(&ch.target_callsign));
            src.do_not_merge = true;
        }
        self.file.peers.push(clone);
        self.flush()?;
        Ok(new_id)
    }

    /// Promote an endpoint to `Operator` provenance IN PLACE — the ONLY path
    /// that writes `Operator` on an endpoint (the keyring secret keyed on the
    /// endpoint id is never orphaned [R5-5]). Flushes on success.
    pub fn promote_endpoint(&mut self, peer_id: &str, endpoint_id: &str) -> Result<(), PeersError> {
        let pidx = self
            .file
            .peers
            .iter()
            .position(|p| p.id == peer_id)
            .ok_or_else(|| PeersError::Validation(format!("peer {peer_id:?} not found")))?;
        let eidx = self.file.peers[pidx]
            .endpoints
            .iter()
            .position(|e| e.id == endpoint_id)
            .ok_or_else(|| PeersError::Validation(format!("endpoint {endpoint_id:?} not found")))?;
        self.file.peers[pidx].endpoints[eidx].provenance = Provenance::Operator;
        self.flush()
    }
}

/// Channel dedup-key equality: `(transport, target_callsign, via, freq_hz,
/// bandwidth)` [R4-11].
fn channel_key_eq(a: &Channel, b: &Channel) -> bool {
    a.transport == b.transport
        && a.target_callsign == b.target_callsign
        && a.via == b.via
        && a.freq_hz == b.freq_hz
        && a.bandwidth == b.bandwidth
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::peers::model::*;
    use crate::peers::recorder::{ObservationPhase, ObservedPath, PeerObservation};

    fn td() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }
    fn now() -> String {
        "2026-07-10T12:00:00-07:00".to_string()
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

    fn manual_peer(id: &str, base: &str) -> Peer {
        Peer {
            id: id.into(),
            canonical_base: base.into(),
            presented_callsigns: vec![base.into()],
            identity_kind: IdentityKind::Unknown,
            do_not_merge: false,
            conflict: false,
            source: RecordSource::Manual,
            origin: Origin::Manual,
            contact_id: None,
            grid: None,
            note: String::new(),
            created_at: "2026-07-10T12:00:00-07:00".into(),
            last_connected_at: None,
            channels: vec![],
            endpoints: vec![],
        }
    }

    #[test]
    fn upserts_by_canonical_base_and_keeps_presented_forms() {
        let dir = td();
        let mut s = PeersStore::open(dir.path().join("peers.json"));
        s.apply_observation(&rf_obs("W6ABC-7", Direction::Outgoing, ObservationPhase::B2fOk), now())
            .unwrap();
        s.apply_observation(&rf_obs("w6abc", Direction::Incoming, ObservationPhase::Accepted), now())
            .unwrap();
        let f = s.file();
        assert_eq!(f.peers.len(), 1, "same base → one record");
        assert_eq!(f.peers[0].canonical_base, "W6ABC");
        assert!(f.peers[0].presented_callsigns.contains(&"W6ABC-7".to_string()));
        assert!(f.peers[0].presented_callsigns.contains(&"W6ABC".to_string()));
        // Two distinct channels (different target_callsign in the key).
        assert_eq!(f.peers[0].channels.len(), 2);
    }

    #[test]
    fn channel_key_distinguishes_via_freq_and_bandwidth() {
        let dir = td();
        let mut s = PeersStore::open(dir.path().join("peers.json"));
        let mut o1 = rf_obs("W6ABC", Direction::Outgoing, ObservationPhase::B2fOk);
        s.apply_observation(&o1, now()).unwrap();
        s.apply_observation(&o1, now()).unwrap(); // same key → counts, not a new row
        assert_eq!(s.file().peers[0].channels.len(), 1);
        assert_eq!(s.file().peers[0].channels[0].counts.ok, 2);
        if let ObservedPath::Rf { ref mut via, .. } = o1.path {
            *via = vec!["DIGI1".into()];
        }
        s.apply_observation(&o1, now()).unwrap(); // different via → distinct channel [R3-6]
        assert_eq!(s.file().peers[0].channels.len(), 2);
    }

    #[test]
    fn split_records_route_by_exact_presented_callsign() {
        // [R5-4]: after a split, base-anchored routing is OFF for that base.
        let dir = td();
        let mut s = PeersStore::open(dir.path().join("peers.json"));
        s.apply_observation(&rf_obs("W6ABC-7", Direction::Outgoing, ObservationPhase::B2fOk), now())
            .unwrap();
        s.apply_observation(&rf_obs("W6ABC-9", Direction::Outgoing, ObservationPhase::B2fOk), now())
            .unwrap();
        let id = s.file().peers[0].id.clone();
        let new_id = s.split(&id, vec!["W6ABC-9".to_string()], now()).unwrap();
        assert!(s.file().peers.iter().all(|p| p.do_not_merge));
        // A new -9 observation routes to the split record…
        s.apply_observation(&rf_obs("W6ABC-9", Direction::Incoming, ObservationPhase::Accepted), now())
            .unwrap();
        let split_rec = s.file().peers.iter().find(|p| p.id == new_id).unwrap();
        assert!(split_rec.channels.iter().any(|c| c.direction == Direction::Incoming));
        // …and an unmatched presented form is held as a conflict record.
        let eff = s
            .apply_observation(
                &rf_obs("W6ABC-11", Direction::Incoming, ObservationPhase::Accepted),
                now(),
            )
            .unwrap();
        assert!(matches!(eff, ApplyEffect::ConflictHeld));
        assert!(s.file().peers.iter().any(|p| p.conflict));
    }

    #[test]
    fn rejected_inbound_never_populates_the_roster() {
        let dir = td();
        let mut s = PeersStore::open(dir.path().join("peers.json"));
        let eff = s
            .apply_observation(&rf_obs("EVIL-1", Direction::Incoming, ObservationPhase::Rejected), now())
            .unwrap();
        assert!(matches!(eff, ApplyEffect::NoRecord));
        assert!(s.file().peers.is_empty(), "an attacker knocking is not a peer");
    }

    #[test]
    fn hostile_callsigns_never_reach_the_roster() {
        // [R2-S2][R2-S10] Task 18 pin: the write boundary gates on
        // `validate_presented_callsign` (Task 8's backstop, NOT
        // `sanitize_display`) — every injection shape below fails that
        // validator too (it accepts only base + optional `/SUFFIX` + optional
        // SSID), so it is dropped as `NoRecord` before any roster write.
        let dir = td();
        let mut s = PeersStore::open(dir.path().join("peers.json"));
        for evil in [
            "<img src=x onerror=alert(1)>",
            "W6ABC:extra",
            "A\u{0}B",
            "../../etc/passwd",
            "W6 ABC",
            "`rm -rf`",
        ] {
            let eff = s
                .apply_observation(
                    &rf_obs(evil, Direction::Incoming, ObservationPhase::Accepted),
                    now(),
                )
                .unwrap();
            assert!(matches!(eff, ApplyEffect::NoRecord), "{evil:?} must be dropped");
        }
        assert!(s.file().peers.is_empty());
    }

    #[test]
    fn wedged_or_aborted_records_a_fail() {
        let dir = td();
        let mut s = PeersStore::open(dir.path().join("peers.json"));
        s.apply_observation(
            &rf_obs("W6ABC", Direction::Outgoing, ObservationPhase::AbortedOrWedged),
            now(),
        )
        .unwrap();
        assert_eq!(s.file().peers[0].channels[0].counts.fail, 1);
        assert_eq!(s.file().peers[0].channels[0].counts.ok, 0);
    }

    #[test]
    fn endpoint_provenance_is_monotonic() {
        // [R4-4][R2-S8]: an inbound observation may never create or mutate an
        // Operator endpoint; only promote_endpoint sets Operator, in place.
        let dir = td();
        let mut s = PeersStore::open(dir.path().join("peers.json"));
        let obs = PeerObservation {
            path: ObservedPath::Telnet {
                host: "203.0.113.5".into(),
                port: 8772,
                provenance: Provenance::ObservedIncoming,
            },
            direction: Direction::Incoming,
            presented_target: "W6ABC".into(),
            phase: ObservationPhase::Accepted,
        };
        s.apply_observation(&obs, now()).unwrap();
        let (pid, eid, prov) = {
            let p = &s.file().peers[0];
            (p.id.clone(), p.endpoints[0].id.clone(), p.endpoints[0].provenance)
        };
        assert_eq!(prov, Provenance::ObservedIncoming);
        s.promote_endpoint(&pid, &eid).unwrap();
        assert_eq!(s.file().peers[0].endpoints[0].provenance, Provenance::Operator);
        assert_eq!(s.file().peers[0].endpoints[0].id, eid, "promotion is in-place [R5-5]");
        // A later ObservedIncoming observation of the same host:port must NOT
        // touch the Operator endpoint (distinct provenance in the key).
        s.apply_observation(&obs, now()).unwrap();
        let p = &s.file().peers[0];
        assert_eq!(
            p.endpoints.iter().filter(|e| e.provenance == Provenance::Operator).count(),
            1
        );
    }

    #[test]
    fn outbound_operator_dial_records_operator_endpoint() {
        // Amendment (a): the Operator→ObservedIncoming downgrade is conditioned
        // on direction == Incoming. An OUTBOUND operator dial legitimately
        // records an Operator endpoint (how a telnet favorite is born, Task 16).
        let dir = td();
        let mut s = PeersStore::open(dir.path().join("peers.json"));
        let obs = PeerObservation {
            path: ObservedPath::Telnet {
                host: "cms.example.org".into(),
                port: 8772,
                provenance: Provenance::Operator,
            },
            direction: Direction::Outgoing,
            presented_target: "W6ABC".into(),
            phase: ObservationPhase::Accepted,
        };
        s.apply_observation(&obs, now()).unwrap();
        assert_eq!(s.file().peers[0].endpoints[0].provenance, Provenance::Operator);
    }

    #[test]
    fn slash_p_presented_form_is_stored() {
        // [R3-F2]: the write boundary gates on validate_presented_callsign, so
        // a legit portable form survives rather than being dropped.
        let dir = td();
        let mut s = PeersStore::open(dir.path().join("peers.json"));
        let eff = s
            .apply_observation(&rf_obs("W6ABC/P", Direction::Outgoing, ObservationPhase::B2fOk), now())
            .unwrap();
        assert!(matches!(eff, ApplyEffect::CreatedPeer));
        assert!(s.file().peers[0].presented_callsigns.contains(&"W6ABC/P".to_string()));
        assert_eq!(s.file().peers[0].canonical_base, "W6ABC");
    }

    #[test]
    fn conflict_creation_is_distinguishable_for_the_limiter_seam() {
        // The Task 9 limiter needs to gate conflict CREATION specifically; the
        // seam must be true only for a split-base unmatched observation.
        let dir = td();
        let mut s = PeersStore::open(dir.path().join("peers.json"));
        // Non-split base: never a conflict creation.
        assert!(!s.would_create_conflict_record(&rf_obs(
            "W6ABC-7",
            Direction::Incoming,
            ObservationPhase::Accepted
        )));
        s.apply_observation(&rf_obs("W6ABC-7", Direction::Outgoing, ObservationPhase::B2fOk), now())
            .unwrap();
        s.apply_observation(&rf_obs("W6ABC-9", Direction::Outgoing, ObservationPhase::B2fOk), now())
            .unwrap();
        let id = s.file().peers[0].id.clone();
        s.split(&id, vec!["W6ABC-9".to_string()], now()).unwrap();
        // Split base, matched form → routes to a twin, not a conflict.
        assert!(!s.would_create_conflict_record(&rf_obs(
            "W6ABC-9",
            Direction::Incoming,
            ObservationPhase::Accepted
        )));
        // Split base, unmatched form → IS a conflict creation (limiter gates).
        assert!(s.would_create_conflict_record(&rf_obs(
            "W6ABC-11",
            Direction::Incoming,
            ObservationPhase::Accepted
        )));
        // Rejected phase → NoRecord, never a conflict creation.
        assert!(!s.would_create_conflict_record(&rf_obs(
            "W6ABC-11",
            Direction::Incoming,
            ObservationPhase::Rejected
        )));
    }

    #[test]
    fn conflict_flood_evicts_only_conflicts_and_spares_real_peers() {
        // R3-F4: a conflict flood on a split base is bounded by CONFLICT_CAP,
        // evicting oldest conflicts only — the two real peers are untouched.
        let dir = td();
        let mut s = PeersStore::open(dir.path().join("peers.json"));
        s.apply_observation(&rf_obs("W6ABC-7", Direction::Outgoing, ObservationPhase::B2fOk), now())
            .unwrap();
        s.apply_observation(&rf_obs("W6ABC-9", Direction::Outgoing, ObservationPhase::B2fOk), now())
            .unwrap();
        let id = s.file().peers[0].id.clone();
        s.split(&id, vec!["W6ABC-9".to_string()], now()).unwrap();
        assert_eq!(s.file().peers.iter().filter(|p| !p.conflict).count(), 2);

        // Flood 150 distinct unmatched presented forms (valid portable suffixes)
        // with ascending timestamps so eviction has a well-defined oldest.
        for i in 0..150u32 {
            let form = format!("W6ABC/Q{i}");
            let ts = format!("2026-07-10T12:{:02}:{:02}-07:00", i / 60, i % 60);
            let eff = s
                .apply_observation(&rf_obs(&form, Direction::Incoming, ObservationPhase::Accepted), ts)
                .unwrap();
            assert!(matches!(eff, ApplyEffect::ConflictHeld));
        }
        let conflicts = s.file().peers.iter().filter(|p| p.conflict).count();
        assert_eq!(conflicts, CONFLICT_CAP, "flood fills exactly to the cap");
        assert_eq!(
            s.file().peers.iter().filter(|p| !p.conflict).count(),
            2,
            "real peers survive the conflict flood"
        );
    }

    #[test]
    fn per_peer_channel_cap_holds() {
        // R3-F4: rotated observations (distinct via) on ONE peer cannot grow it
        // without bound — the per-peer channel cap holds.
        let dir = td();
        let mut s = PeersStore::open(dir.path().join("peers.json"));
        for i in 0..80u32 {
            let mut o = rf_obs("W6ABC", Direction::Outgoing, ObservationPhase::B2fOk);
            if let ObservedPath::Rf { ref mut via, .. } = o.path {
                *via = vec![format!("DIGI{i}")];
            }
            let ts = format!("2026-07-10T12:{:02}:{:02}-07:00", i / 60, i % 60);
            s.apply_observation(&o, ts).unwrap();
        }
        assert_eq!(s.file().peers.len(), 1, "all observations on one peer");
        assert_eq!(s.file().peers[0].channels.len(), PER_PEER_CHANNEL_CAP);
    }

    #[test]
    fn corrupt_file_quarantines_and_starts_empty() {
        let dir = td();
        let path = dir.path().join("peers.json");
        std::fs::write(&path, b"{ not json").unwrap();
        let s = PeersStore::open(path.clone());
        assert!(s.file().peers.is_empty());
        let quarantined = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .any(|e| e.file_name().to_string_lossy().contains("corrupt"));
        assert!(quarantined, "original bytes preserved");
    }

    #[test]
    fn atomic_write_round_trips() {
        let dir = td();
        let path = dir.path().join("peers.json");
        let mut s = PeersStore::open(path.clone());
        s.apply_observation(&rf_obs("W6ABC-7", Direction::Outgoing, ObservationPhase::B2fOk), now())
            .unwrap();
        let reopened = PeersStore::open(path);
        assert_eq!(reopened.file().peers.len(), 1);
        assert_eq!(reopened.file().peers[0].canonical_base, "W6ABC");
    }

    #[test]
    fn manual_upsert_accepts_portable_form_and_rejects_garbage() {
        // [R3-F2] / Task-11 amendment: the manual write boundary gates on
        // validate_presented_callsign, so a hand-entered `W6ABC/P` is stored
        // (matching the observation path), while an injection-y string is
        // rejected.
        let dir = td();
        let mut s = PeersStore::open(dir.path().join("peers.json"));
        let mut p = manual_peer("p-port", "W6ABC");
        p.presented_callsigns = vec!["W6ABC/P".to_string()];
        s.upsert_manual(p).unwrap();
        assert!(s.file().peers[0]
            .presented_callsigns
            .contains(&"W6ABC/P".to_string()));

        let mut bad = manual_peer("p-bad", "W6ABC");
        bad.presented_callsigns = vec!["../etc/passwd".to_string()];
        assert!(matches!(
            s.upsert_manual(bad),
            Err(PeersError::Validation(_))
        ));
    }

    #[test]
    fn merge_absorbs_channels_endpoints_and_presented_forms() {
        let dir = td();
        let mut s = PeersStore::open(dir.path().join("peers.json"));
        s.upsert_manual(manual_peer("p-keep", "W6ABC")).unwrap();
        s.upsert_manual(manual_peer("p-absorb", "W6ABC")).unwrap();
        let dispositions = s.merge("p-keep", "p-absorb").unwrap();
        assert_eq!(s.file().peers.len(), 1);
        assert_eq!(s.file().peers[0].id, "p-keep");
        assert!(dispositions.is_empty(), "no endpoints on either record");
    }

    #[test]
    fn merge_reports_survived_vs_deduped_endpoint_dispositions() {
        // The peer_merge command's keyring cascade consumes these: a Survived
        // endpoint's secret is re-keyed to keep_id; a Deduped endpoint's secret
        // is deleted (its id exists nowhere after the merge).
        fn ep(id: &str, host: &str, port: u16) -> Endpoint {
            Endpoint {
                id: id.into(),
                host: host.into(),
                port,
                provenance: Provenance::Operator,
                last_seen: "2026-07-10T12:00:00-07:00".into(),
            }
        }
        let dir = td();
        let mut s = PeersStore::open(dir.path().join("peers.json"));
        let mut keep = manual_peer("p-keep", "W6ABC");
        keep.endpoints = vec![ep("e-keep", "shared.example.org", 8772)];
        let mut absorb = manual_peer("p-absorb", "W6ABC");
        absorb.endpoints = vec![
            // Same (host, port, provenance) as e-keep → deduped away.
            ep("e-dup", "shared.example.org", 8772),
            // Unique → survives under p-keep with its original id.
            ep("e-uniq", "uniq.example.org", 8772),
        ];
        s.upsert_manual(keep).unwrap();
        s.upsert_manual(absorb).unwrap();

        let dispositions = s.merge("p-keep", "p-absorb").unwrap();
        assert_eq!(
            dispositions,
            vec![
                AbsorbedEndpoint::Deduped {
                    endpoint_id: "e-dup".into()
                },
                AbsorbedEndpoint::Survived {
                    endpoint_id: "e-uniq".into()
                },
            ]
        );
        // The survived endpoint kept its id, now under p-keep.
        let kept = &s.file().peers[0];
        assert_eq!(kept.id, "p-keep");
        assert!(kept.endpoints.iter().any(|e| e.id == "e-uniq"));
        assert!(!kept.endpoints.iter().any(|e| e.id == "e-dup"));
    }
}
