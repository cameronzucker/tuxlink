//! Shared contact-observation recorder (spec §3 + §AMENDMENT). There is NO
//! single chokepoint [R4-1] — each transport calls this from its own
//! attempt-conclusion site(s), via `ObservationGuard` so wedged /
//! aborted / early-return paths still record a fail [R3-11].
//!
//! Moved from the deleted `peers/recorder.rs` (contacts-superset pivot
//! 2026-07-10/11) with the SAME ObservationGuard drop-guard contract and
//! observation types — the record sites changed import path and target
//! store, not semantics. What changed underneath: observations now route to
//! the CONTACTS store by **exact presented-callsign match only** (an
//! observation attaches to a contact iff the callsign matches exactly;
//! otherwise it creates an `Unconfirmed` contact — no base-normalization
//! merging, spec §AMENDMENT pt. 4), and a real write emits
//! `contacts:changed` (the `peers:changed` event died with the peers store).

use crate::contacts::reachability::{ChannelBandwidth, ChannelTransport, Direction, Provenance};
use std::sync::{Arc, Mutex, RwLock};

/// The conclusion phase of a single connection attempt. The recorder maps
/// these to a [`Classified`] bucket via [`classify`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObservationPhase {
    DialAttempted,
    Connected,
    LoginFailed,
    B2fStarted,
    B2fOk,
    B2fFail,
    Accepted,
    Rejected,
    AbortedOrWedged,
}

/// The store-facing classification of a phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Classified {
    Ok,
    Fail,
    /// Rejected/unauthorized inbound: an attacker knocking is not a contact.
    NoRecord,
}

/// Map an attempt-conclusion phase to its roster effect. Pure — the store's
/// single source of truth for "does this observation touch the roster, and as
/// an ok or a fail?".
pub fn classify(phase: ObservationPhase) -> Classified {
    match phase {
        ObservationPhase::B2fOk | ObservationPhase::Accepted => Classified::Ok,
        ObservationPhase::Rejected => Classified::NoRecord,
        ObservationPhase::DialAttempted
        | ObservationPhase::Connected
        | ObservationPhase::LoginFailed
        | ObservationPhase::B2fStarted
        | ObservationPhase::B2fFail
        | ObservationPhase::AbortedOrWedged => Classified::Fail,
    }
}

/// The reachability path an observation was made over.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObservedPath {
    Rf {
        transport: ChannelTransport,
        via: Vec<String>,
        /// Incoming rows have no wire freq source (CONNECTED carries
        /// bandwidth, not frequency) — rig/CAT state if available, else
        /// None; never fabricated [R3-11].
        freq_hz: Option<u64>,
        bandwidth: Option<ChannelBandwidth>,
    },
    Telnet {
        host: String,
        port: u16,
        provenance: Provenance,
    },
}

/// One connection-attempt observation, routed to the roster by the store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerObservation {
    pub path: ObservedPath,
    pub direction: Direction,
    /// Exact presented/SSID'd callsign of the far station.
    pub presented_target: String,
    pub phase: ObservationPhase,
}

/// A recorder sink: the effectful tail that turns a concluded
/// [`PeerObservation`] into a roster write. In production this is a closure
/// captured over the app's `ContactsStore` + `InboundCreateLimiter` state
/// that calls [`record_contact_observation`] and emits `contacts:changed` on
/// a real write; tests substitute a capturing closure.
pub type ObservationSink = Arc<dyn Fn(PeerObservation) + Send + Sync>;

/// Process-global observation sink [R4-1]. The record sites span backend
/// layers with and without an `AppHandle` (`native_packet_connect` and
/// `telnet_listen::handle_one_session` have none), so rather than thread a
/// sink parameter through the whole backend, the recorder holds it here.
/// Installed ONCE at app setup (`lib.rs`), after the contacts store + limiter
/// are managed; every site reads it via [`observation_sink`] and is a NO-OP
/// when `None` (unit tests, headless tools).
static SINK: RwLock<Option<ObservationSink>> = RwLock::new(None);

/// Install the global observation sink. Called once from `lib.rs` `.setup()`.
/// A poisoned lock leaves the sink uninstalled (sites no-op) rather than panics.
pub fn install_observation_sink(sink: ObservationSink) {
    if let Ok(mut g) = SINK.write() {
        *g = Some(sink);
    }
}

/// The installed sink, or `None` when no sink has been installed (tests,
/// headless tools) or the lock is poisoned. Record sites clone this to arm an
/// [`ObservationGuard`]; a `None` return means the site is a no-op.
pub fn observation_sink() -> Option<ObservationSink> {
    SINK.read().ok().and_then(|g| g.clone())
}

/// Process-global inbound-create limiter handle [R3-F5]. A SECOND global,
/// parallel to [`SINK`]. The allowlist-reject sites (telnet
/// `handle_one_session`, the VARA/ARDOP/packet listener gates) take no
/// `AppHandle`, and a rejected inbound classifies as [`Classified::NoRecord`],
/// so [`record_contact_observation`] bails on it BEFORE ever touching the
/// limiter — routing rejects through the observation sink can therefore never
/// increment the quarantine counter. This handle gives every reject site a
/// zero-plumbing path to the SAME managed
/// [`InboundCreateLimiter`](crate::contacts::limiter::InboundCreateLimiter)
/// the sink captures. Installed ONCE at app setup (`lib.rs`), immediately
/// after the limiter is managed; reject sites are NO-OPS when `None` (unit
/// tests, headless tools).
static INBOUND_LIMITER: RwLock<Option<Arc<Mutex<crate::contacts::limiter::InboundCreateLimiter>>>> =
    RwLock::new(None);

/// Install the global inbound-create limiter handle. Called once from `lib.rs`
/// `.setup()` with the same `Arc<Mutex<InboundCreateLimiter>>` the observation
/// sink captures. A poisoned lock leaves it uninstalled (reject sites no-op).
pub fn install_inbound_limiter(
    limiter: Arc<Mutex<crate::contacts::limiter::InboundCreateLimiter>>,
) {
    if let Ok(mut g) = INBOUND_LIMITER.write() {
        *g = Some(limiter);
    }
}

/// The installed inbound-create limiter, or `None` (tests, headless tools, or a
/// poisoned lock). [`record_inbound_reject`] reads it; a `None` return no-ops.
fn inbound_limiter() -> Option<Arc<Mutex<crate::contacts::limiter::InboundCreateLimiter>>> {
    INBOUND_LIMITER.read().ok().and_then(|g| g.clone())
}

/// Record an allowlist / password / TTL-rejected inbound connection against the
/// process-global limiter's FAILED path [R3-F5]. Creates NO roster record — a
/// rejected inbound is an attacker knocking, not a contact — but it IS the only
/// path by which the spoofing-loop quarantine counter (spec §2 caps) sees real
/// rejected bursts (the accepted-answer guard never does, and a `NoRecord`
/// observation never reaches the limiter through the sink). Every allowlist-
/// reject site calls this: telnet `handle_one_session`, and the VARA / ARDOP /
/// packet listener gates. No `AppHandle` needed — the limiter is a
/// process-global. Emits a visible `tracing::warn!`; a `None` limiter
/// (tests / headless) is a silent no-op.
pub fn record_inbound_reject(transport: ChannelTransport) {
    let Some(limiter) = inbound_limiter() else {
        return;
    };
    let (over_budget, quarantined_total) = match limiter.lock() {
        Ok(mut l) => {
            let over = !l.allow(transport, false, std::time::Instant::now());
            (over, l.quarantined())
        }
        Err(_) => return,
    };
    tracing::warn!(
        target: "tuxlink::contacts",
        transport = ?transport,
        over_budget,
        quarantined_total,
        "inbound connection rejected (allowlist/password/TTL) — counted on the failed-path limiter; no roster record"
    );
}

/// Drop-guard recorder [R3-11]: construct at attempt start (`DialAttempted`, or
/// the `Accepted`-path initial for an inbound), advance the phase via
/// [`ObservationGuard::set_phase`] as the exchange progresses, and the record
/// fires ON DROP with the latest phase — so EVERY exit path (early return, `?`,
/// panic-unwind, or a wedge that merely lets the scope end) still records. The
/// guard IS the `finally`; there is no single chokepoint every transport funnels
/// through [R4-1]. Call [`ObservationGuard::disarm`] when a different site owns
/// this attempt's authoritative record, to suppress the drop-fire.
pub struct ObservationGuard {
    sink: ObservationSink,
    obs: Mutex<Option<PeerObservation>>,
}

impl ObservationGuard {
    /// Arm a guard with its initial observation. The record fires on drop unless
    /// [`disarm`](Self::disarm) is called first.
    pub fn new(sink: ObservationSink, initial: PeerObservation) -> Self {
        Self {
            sink,
            obs: Mutex::new(Some(initial)),
        }
    }

    /// Advance the recorded phase. The LATEST phase set before drop is the one
    /// that fires. A poisoned lock is a no-op (the drop-fire then uses the last
    /// successfully-set phase).
    pub fn set_phase(&self, phase: ObservationPhase) {
        if let Ok(mut g) = self.obs.lock() {
            if let Some(o) = g.as_mut() {
                o.phase = phase;
            }
        }
    }

    /// Update path details learned mid-attempt (e.g. bandwidth parsed from the
    /// CONNECTED line, or a via/freq not known at dial time).
    pub fn set_path(&self, path: ObservedPath) {
        if let Ok(mut g) = self.obs.lock() {
            if let Some(o) = g.as_mut() {
                o.path = path;
            }
        }
    }

    /// Disarm the guard: no record fires on drop. Use when a different site owns
    /// the authoritative record for this attempt.
    pub fn disarm(&self) {
        if let Ok(mut g) = self.obs.lock() {
            g.take();
        }
    }
}

impl Drop for ObservationGuard {
    fn drop(&mut self) {
        if let Ok(mut g) = self.obs.lock() {
            if let Some(o) = g.take() {
                (self.sink)(o);
            }
        }
    }
}

/// Cascade the keyring secrets of endpoints whose (unconfirmed) contacts were
/// LRU-evicted by an observation write (spec §AMENDMENT pt. 8: eviction of an
/// unconfirmed contact cascades its endpoint keyring secrets). Injected
/// `delete` so the cascade is deterministically testable without a real
/// keyring; production passes
/// [`crate::winlink::credentials::p2p_endpoint_password_delete`]. Best-effort:
/// a backend failure warns (account ids only — NEVER a secret value) and
/// continues; the roster write already succeeded.
pub fn cascade_evicted_endpoint_secrets<D>(evicted: &[(String, String)], delete: D)
where
    D: Fn(&str, &str) -> Result<(), String>,
{
    for (contact_id, endpoint_id) in evicted {
        if let Err(e) = delete(contact_id, endpoint_id) {
            tracing::warn!(
                target: "tuxlink::contacts",
                contact_id = %contact_id,
                endpoint_id = %endpoint_id,
                "eviction: clearing endpoint keyring secret failed: {e}"
            );
        }
    }
}

/// Central recorder entry [R4-1]: classification → inbound-create rate limit →
/// store apply → evicted-endpoint keyring cascade. Record-site sinks wrap this
/// with their app state (the managed `ContactsStore` + `InboundCreateLimiter`)
/// and emit `contacts:changed` when the returned effect is a real write.
///
/// Rate-limiting applies to inbound CREATES only [R5-9]: outbound observations
/// and updates to an existing record always pass. The exists-probe mirrors the
/// store's routing exactly — EXACT presented-callsign match (any tier), no
/// base normalization — so a create-shaped inbound can never bypass the
/// limiter through a base-anchored skip.
pub fn record_contact_observation(
    store: &Mutex<crate::contacts::store::ContactsStore>,
    limiter: &Mutex<crate::contacts::limiter::InboundCreateLimiter>,
    obs: PeerObservation,
) -> crate::contacts::store::ApplyEffect {
    use crate::contacts::store::ApplyEffect;
    if classify(obs.phase) == Classified::NoRecord {
        return ApplyEffect::NoRecord;
    }
    // Rate-limit inbound CREATES only [R5-9]: existing-record updates and
    // outbound observations always pass.
    if obs.direction == Direction::Incoming {
        let presented = obs.presented_target.trim().to_ascii_uppercase();
        let exists = store
            .lock()
            .map(|s| {
                s.file()
                    .contacts
                    .iter()
                    .any(|c| c.callsign.trim().eq_ignore_ascii_case(&presented))
            })
            .unwrap_or(false);
        if !exists {
            let transport = match &obs.path {
                ObservedPath::Rf { transport, .. } => *transport,
                ObservedPath::Telnet { .. } => ChannelTransport::Unknown,
            };
            let accepted = classify(obs.phase) == Classified::Ok;
            let allowed = limiter
                .lock()
                .map(|mut l| l.allow(transport, accepted, std::time::Instant::now()))
                .unwrap_or(true);
            if !allowed {
                let q = limiter.lock().map(|l| l.quarantined()).unwrap_or(0);
                tracing::warn!(
                    target: "tuxlink::contacts",
                    presented = %obs.presented_target,
                    quarantined_total = q,
                    "inbound contact auto-create rate-limited — quarantined (not added to the address book)"
                );
                return ApplyEffect::NoRecord;
            }
        }
    }
    let now = chrono::Local::now().to_rfc3339();
    let outcome = match store.lock() {
        Ok(mut s) => s.apply_observation(&obs, now).unwrap_or_else(|e| {
            tracing::warn!(target: "tuxlink::contacts", "contact observation write failed: {e:?}");
            crate::contacts::store::ApplyOutcome::no_record()
        }),
        Err(_) => crate::contacts::store::ApplyOutcome::no_record(),
    };
    // Spec §AMENDMENT pt. 8: LRU eviction of an unconfirmed contact cascades
    // its endpoint keyring secrets.
    cascade_evicted_endpoint_secrets(
        &outcome.evicted_endpoint_secrets,
        crate::winlink::credentials::p2p_endpoint_password_delete,
    );
    outcome.effect
}

/// The ONE bridge from contact observations to the favorites attempt log
/// [R5-7]. The observation recorder is authoritative for P2P recents; this
/// bridge is the sole path by which a concluded P2P attempt ALSO lands an
/// empirical entry in the favorites/Recents log so the mode's Recent tab
/// reflects P2P dials the same way it reflects CMS/gateway dials. Outbound
/// conclusions only — recents = DIALS, so an inbound (`Direction::Incoming`)
/// observation (someone dialing US) bridges nothing. The frontend's ribbon
/// dispatcher (`connectDispatch.ts`) suppresses its own `recordRibbonAttempt`
/// call for `p2p` sessions specifically so this is the ONLY writer for a P2P
/// attempt — a second writer would double-count.
pub fn bridge_to_favorites(
    favorites: &Arc<Mutex<crate::favorites::store::FavoritesStore>>,
    obs: &PeerObservation,
) {
    if obs.direction != Direction::Outgoing {
        return;
    }
    let outcome = match classify(obs.phase) {
        Classified::Ok => "reached",
        Classified::Fail => "failed",
        Classified::NoRecord => return,
    };
    let mode = match &obs.path {
        ObservedPath::Rf { transport, .. } => match transport {
            ChannelTransport::VaraHf => "vara-hf",
            ChannelTransport::VaraFm => "vara-fm",
            ChannelTransport::Ardop => "ardop-hf",
            ChannelTransport::Packet => "packet",
            // Unknown is never a real dialed mode (no favorites/Recent surface
            // for it) — skip rather than mint a bogus "unknown" recent.
            ChannelTransport::Unknown => return,
        },
        ObservedPath::Telnet { .. } => "telnet",
    };
    let ts_local = chrono::Local::now().to_rfc3339();
    let now = chrono::Utc::now().to_rfc3339();
    let dial = crate::favorites::store::FavoriteDial {
        mode: mode.to_string(),
        gateway: obs.presented_target.clone(),
        // The bridge observes only mode/gateway/outcome — freq/transport/band/
        // grid/contact link are not carried by a `PeerObservation` today (it
        // has no wire freq for RF conclusions, H1's freq-on-record-only rule,
        // and no roster lookup here); every field is stated explicitly (the
        // struct has NO `Default`) rather than defaulted implicitly.
        freq: None,
        transport: None,
        band: None,
        grid: None,
        contact_id: None,
    };
    if let Ok(mut f) = favorites.lock() {
        if let Err(e) = f.record_attempt(
            dial,
            outcome.to_string(),
            ts_local,
            || uuid::Uuid::new_v4().to_string(),
            now,
        ) {
            tracing::warn!(target: "tuxlink::contacts", "favorites bridge skipped: {e:?}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    fn obs(phase: ObservationPhase) -> PeerObservation {
        PeerObservation {
            path: ObservedPath::Rf {
                transport: ChannelTransport::VaraHf,
                via: vec![],
                freq_hz: None,
                bandwidth: None,
            },
            direction: Direction::Outgoing,
            presented_target: "W6ABC".into(),
            phase,
        }
    }

    /// Like [`obs`], but with a caller-chosen presented form + direction.
    fn obs_for(presented: &str, direction: Direction, phase: ObservationPhase) -> PeerObservation {
        PeerObservation {
            path: ObservedPath::Rf {
                transport: ChannelTransport::VaraHf,
                via: vec![],
                freq_hz: None,
                bandwidth: None,
            },
            direction,
            presented_target: presented.into(),
            phase,
        }
    }

    #[test]
    fn classification_matches_the_spec_table() {
        // spec §3: dial_attempted → connected → (login_failed | b2f_started
        // → b2f_ok | b2f_fail) | accepted | rejected | aborted/wedged
        assert_eq!(classify(ObservationPhase::B2fOk), Classified::Ok);
        assert_eq!(classify(ObservationPhase::Accepted), Classified::Ok);
        assert_eq!(classify(ObservationPhase::Rejected), Classified::NoRecord);
        for p in [
            ObservationPhase::DialAttempted,
            ObservationPhase::Connected,
            ObservationPhase::LoginFailed,
            ObservationPhase::B2fStarted,
            ObservationPhase::B2fFail,
            ObservationPhase::AbortedOrWedged,
        ] {
            assert_eq!(classify(p), Classified::Fail, "{p:?}");
        }
    }

    #[test]
    fn guard_fires_on_drop_with_the_latest_phase() {
        let seen: Arc<Mutex<Vec<PeerObservation>>> = Arc::default();
        let sink = {
            let seen = seen.clone();
            Arc::new(move |o: PeerObservation| seen.lock().unwrap().push(o))
        };
        {
            let g = ObservationGuard::new(sink.clone(), obs(ObservationPhase::DialAttempted));
            g.set_phase(ObservationPhase::Connected);
            g.set_phase(ObservationPhase::B2fOk);
        } // drop → fire
        assert_eq!(seen.lock().unwrap().len(), 1);
        assert_eq!(seen.lock().unwrap()[0].phase, ObservationPhase::B2fOk);
    }

    #[test]
    fn guard_records_fail_when_dropped_mid_exchange() {
        // The ARDOP-ARQTimeout lesson [R3-11]: a wedge/abort/early-return
        // path still records — the guard IS the finally.
        let seen: Arc<Mutex<Vec<PeerObservation>>> = Arc::default();
        let sink = {
            let seen = seen.clone();
            Arc::new(move |o: PeerObservation| seen.lock().unwrap().push(o))
        };
        {
            let g = ObservationGuard::new(sink, obs(ObservationPhase::DialAttempted));
            g.set_phase(ObservationPhase::Connected);
            // …exchange wedges; nothing sets B2fOk…
        }
        assert_eq!(classify(seen.lock().unwrap()[0].phase), Classified::Fail);
    }

    #[test]
    fn guard_disarm_suppresses_the_record() {
        let seen: Arc<Mutex<Vec<PeerObservation>>> = Arc::default();
        let sink = {
            let seen = seen.clone();
            Arc::new(move |o: PeerObservation| seen.lock().unwrap().push(o))
        };
        {
            let g = ObservationGuard::new(sink, obs(ObservationPhase::DialAttempted));
            g.disarm(); // another site owns this attempt's record
        }
        assert!(seen.lock().unwrap().is_empty());
    }

    #[test]
    #[serial_test::serial]
    fn global_sink_install_read_and_fire_roundtrip() {
        // The process-global sink: install → observation_sink() returns it →
        // an ObservationGuard armed with it fires into the captured buffer on
        // drop. #[serial] because SINK is a process-global RwLock.
        let seen: Arc<Mutex<Vec<PeerObservation>>> = Arc::default();
        {
            let seen = seen.clone();
            install_observation_sink(Arc::new(move |o| seen.lock().unwrap().push(o)));
        }
        let sink = observation_sink().expect("sink is installed → Some");
        {
            let g = ObservationGuard::new(sink, obs(ObservationPhase::DialAttempted));
            g.set_phase(ObservationPhase::B2fOk);
        } // drop → fire into the installed sink
        assert_eq!(seen.lock().unwrap().len(), 1);
        assert_eq!(seen.lock().unwrap()[0].phase, ObservationPhase::B2fOk);
        install_observation_sink(Arc::new(|_| {})); // reset to a no-op sink
    }

    #[test]
    #[serial_test::serial]
    fn record_inbound_reject_drives_the_quarantine_counter() {
        // R3-F5: N rejects on the failed path increment `quarantined()`. This is
        // the process-global fn every allowlist-reject site calls. Default
        // `failed_per_minute` = 10, so 40 rejects → 10 counted, 30 quarantined.
        // #[serial] because INBOUND_LIMITER is a process-global RwLock shared
        // with the observation-sink tests.
        use crate::contacts::limiter::{InboundCreateLimiter, P2pLimitsConfig};
        let limiter = Arc::new(Mutex::new(InboundCreateLimiter::new(P2pLimitsConfig::default())));
        install_inbound_limiter(limiter.clone());
        for _ in 0..40u32 {
            record_inbound_reject(ChannelTransport::Unknown);
        }
        assert_eq!(
            limiter.lock().unwrap().quarantined(),
            30,
            "40 rejects − failed_per_minute(10) = 30 quarantined"
        );
        // Restore a fresh limiter so a later serial test starts from a clean
        // counter.
        install_inbound_limiter(Arc::new(Mutex::new(InboundCreateLimiter::new(
            P2pLimitsConfig::default(),
        ))));
    }

    #[test]
    fn inbound_create_flood_is_quarantined_through_the_central_entry() {
        // End-to-end proof that record_contact_observation runs inbound
        // CREATES through the limiter: a burst of 40 distinct never-seen
        // callsigns on the accepted path is capped at accepted_per_hour;
        // everything over budget returns NoRecord and never reaches the
        // address book. (Small custom budget keeps the test fast.)
        use crate::contacts::limiter::{InboundCreateLimiter, P2pLimitsConfig};
        use crate::contacts::store::{ApplyEffect, ContactsStore};

        let dir = tempfile::tempdir().unwrap();
        let store = Mutex::new(ContactsStore::open(dir.path().join("contacts.json")));
        let limiter = Mutex::new(InboundCreateLimiter::new(P2pLimitsConfig {
            accepted_per_hour: 5,
            failed_per_minute: 10,
        }));

        let (mut created, mut quarantined) = (0u32, 0u32);
        for i in 0..40u32 {
            let o = obs_for(
                &format!("K{i}ABC"),
                Direction::Incoming,
                ObservationPhase::Accepted,
            );
            match record_contact_observation(&store, &limiter, o) {
                ApplyEffect::CreatedContact => created += 1,
                ApplyEffect::NoRecord => quarantined += 1,
                other => panic!("unexpected effect {other:?}"),
            }
        }
        assert_eq!(created, 5, "accepted budget gates auto-creation");
        assert_eq!(quarantined, 35, "everything over budget is quarantined");
        let s = store.lock().unwrap();
        assert_eq!(s.file().contacts.len(), 5, "the roster holds only the budget");
        assert_eq!(limiter.lock().unwrap().quarantined(), 35);
    }

    #[test]
    fn inbound_update_to_an_existing_contact_is_never_limited() {
        // [R5-9] existing-record updates always pass: with a ZERO accepted
        // budget, an inbound observation whose exact callsign already exists
        // still updates the record (only CREATES are limited).
        use crate::contacts::limiter::{InboundCreateLimiter, P2pLimitsConfig};
        use crate::contacts::store::{ApplyEffect, ContactsStore};

        let dir = tempfile::tempdir().unwrap();
        let store = Mutex::new(ContactsStore::open(dir.path().join("contacts.json")));
        // Seed via an OUTBOUND observation (never limited).
        let seeded = record_contact_observation(
            &store,
            &Mutex::new(InboundCreateLimiter::new(P2pLimitsConfig::default())),
            obs_for("W6ABC-7", Direction::Outgoing, ObservationPhase::B2fOk),
        );
        assert_eq!(seeded, ApplyEffect::CreatedContact);

        let zero = Mutex::new(InboundCreateLimiter::new(P2pLimitsConfig {
            accepted_per_hour: 0,
            failed_per_minute: 0,
        }));
        let eff = record_contact_observation(
            &store,
            &zero,
            obs_for("W6ABC-7", Direction::Incoming, ObservationPhase::Accepted),
        );
        assert_eq!(eff, ApplyEffect::UpdatedContact, "updates bypass the limiter");
        assert_eq!(zero.lock().unwrap().quarantined(), 0);
    }

    #[test]
    fn cascade_helper_deletes_each_evicted_pair_and_survives_failures() {
        use std::cell::RefCell;
        let deleted: RefCell<Vec<(String, String)>> = RefCell::new(vec![]);
        let evicted = vec![
            ("c1".to_string(), "e1".to_string()),
            ("c1".to_string(), "e2".to_string()),
            ("c2".to_string(), "e3".to_string()),
        ];
        cascade_evicted_endpoint_secrets(&evicted, |c, e| {
            if e == "e2" {
                return Err("simulated backend failure".to_string());
            }
            deleted.borrow_mut().push((c.to_string(), e.to_string()));
            Ok(())
        });
        // The failure on e2 did not abort the cascade — e3 still deleted.
        assert_eq!(
            *deleted.borrow(),
            vec![
                ("c1".to_string(), "e1".to_string()),
                ("c2".to_string(), "e3".to_string()),
            ]
        );
    }

    // ---- bridge_to_favorites [R5-7] ------------------------------------------

    fn rf_obs(transport: ChannelTransport, direction: Direction, phase: ObservationPhase) -> PeerObservation {
        PeerObservation {
            path: ObservedPath::Rf { transport, via: vec![], freq_hz: None, bandwidth: None },
            direction,
            presented_target: "W6ABC".into(),
            phase,
        }
    }

    fn telnet_obs(direction: Direction, phase: ObservationPhase) -> PeerObservation {
        PeerObservation {
            path: ObservedPath::Telnet {
                host: "cms.example".into(),
                port: 8772,
                provenance: Provenance::default(),
            },
            direction,
            presented_target: "W6ABC".into(),
            phase,
        }
    }

    #[test]
    fn bridge_maps_transport_to_radio_mode() {
        use crate::favorites::store::FavoritesStore;

        let cases: [(ChannelTransport, &str); 4] = [
            (ChannelTransport::VaraHf, "vara-hf"),
            (ChannelTransport::VaraFm, "vara-fm"),
            (ChannelTransport::Ardop, "ardop-hf"),
            (ChannelTransport::Packet, "packet"),
        ];
        for (transport, expected_mode) in cases {
            let dir = tempfile::tempdir().unwrap();
            let favorites = Arc::new(Mutex::new(FavoritesStore::open(
                dir.path().join("stations.json"),
            )));
            let o = rf_obs(transport, Direction::Outgoing, ObservationPhase::B2fOk);
            bridge_to_favorites(&favorites, &o);
            let f = favorites.lock().unwrap();
            let favs = f.favorites();
            assert_eq!(favs.len(), 1, "{transport:?} must bridge exactly one recent");
            assert_eq!(favs[0].mode, expected_mode, "{transport:?} → {expected_mode}");
        }

        // Telnet path → "telnet".
        let dir = tempfile::tempdir().unwrap();
        let favorites = Arc::new(Mutex::new(FavoritesStore::open(
            dir.path().join("stations.json"),
        )));
        let o = telnet_obs(Direction::Outgoing, ObservationPhase::B2fOk);
        bridge_to_favorites(&favorites, &o);
        let f = favorites.lock().unwrap();
        assert_eq!(f.favorites()[0].mode, "telnet");
    }

    #[test]
    fn bridge_appends_one_reached_attempt_for_an_outbound_b2fok() {
        use crate::favorites::store::FavoritesStore;

        let dir = tempfile::tempdir().unwrap();
        let favorites = Arc::new(Mutex::new(FavoritesStore::open(
            dir.path().join("stations.json"),
        )));
        let o = rf_obs(ChannelTransport::VaraHf, Direction::Outgoing, ObservationPhase::B2fOk);
        bridge_to_favorites(&favorites, &o);

        let f = favorites.lock().unwrap();
        assert_eq!(f.favorites().len(), 1, "one recent created");
        let unit_id = f.favorites()[0].id.clone();
        let attempts = f.attempts_for(&unit_id);
        assert_eq!(attempts.len(), 1, "exactly one attempt appended");
        assert_eq!(attempts[0].outcome, "reached");
    }

    #[test]
    fn bridge_ignores_incoming_observations() {
        // Recents = DIALS: an inbound (someone dialing US) must bridge nothing,
        // even a successful B2fOk.
        use crate::favorites::store::FavoritesStore;

        let dir = tempfile::tempdir().unwrap();
        let favorites = Arc::new(Mutex::new(FavoritesStore::open(
            dir.path().join("stations.json"),
        )));
        let o = rf_obs(ChannelTransport::VaraHf, Direction::Incoming, ObservationPhase::B2fOk);
        bridge_to_favorites(&favorites, &o);

        let f = favorites.lock().unwrap();
        assert!(f.favorites().is_empty(), "inbound observations must not bridge");
        assert!(f.log().is_empty());
    }
}
