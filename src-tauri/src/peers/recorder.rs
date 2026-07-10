//! Shared peer-observation recorder (spec §3). There is NO single
//! chokepoint [R4-1] — each transport calls this from its own
//! attempt-conclusion site(s), via `ObservationGuard` so wedged /
//! aborted / early-return paths still record a fail [R3-11].
//!
//! **Task scoping:** the observation TYPES + pure phase-classifier are consumed
//! by the peer store (Task 8) and the inbound limiter (Task 9). Task 11 adds
//! the [`ObservationGuard`] drop-guard and the central [`record_peer_observation`]
//! entry point (classification → inbound-create rate limit → store apply).

use crate::peers::model::{ChannelBandwidth, ChannelTransport, Direction, Provenance};
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
    /// Rejected/unauthorized inbound: an attacker knocking is not a peer.
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
/// captured over the app's `PeersStore` + `InboundCreateLimiter` state that
/// calls [`record_peer_observation`] and emits `peers:changed` on a real write;
/// tests substitute a capturing closure.
pub type ObservationSink = Arc<dyn Fn(PeerObservation) + Send + Sync>;

/// Process-global observation sink [R4-1]. The 8 record sites (Tasks 13-16) span
/// backend layers with and without an `AppHandle` (`native_packet_connect` and
/// `telnet_listen::handle_one_session` have none), so rather than thread a sink
/// parameter through the whole backend, the recorder holds it here. Installed
/// ONCE at app setup (`lib.rs`), after the peers store + limiter are managed;
/// every site reads it via [`observation_sink`] and is a NO-OP when `None`
/// (unit tests, headless tools).
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

/// Central recorder entry [R4-1]: classification → inbound-create rate limit →
/// store apply, with visible quarantine logging. Record-site sinks wrap this
/// with their app state (the managed `PeersStore` + `InboundCreateLimiter`) and
/// emit `peers:changed` when the returned effect is a real write.
///
/// Rate-limiting applies to inbound CREATES only [R5-9]: outbound observations
/// and updates to an existing record always pass. Two create-shaped cases are
/// gated:
///
/// 1. A brand-new base (no roster record yet) runs the accepted/failed
///    threshold.
/// 2. An unmatched presented form on an operator-SPLIT base — which WOULD create
///    a held `conflict` record — runs the failed-path budget via
///    [`crate::peers::limiter::InboundCreateLimiter::allow_conflict_creation`].
///    A split base always "exists", so without this second gate the base-exists
///    skip below would let a conflict flood bypass the limiter entirely (limiter
///    amendment (b)(ii)).
///
/// **Latent limiter bypass — heads-up for the task that introduces Tactical
/// assignment.** The `exists` probe below matches on `canonical_base`, but a
/// `Tactical` record ANCHORS on its full presented string [R4-6]: once an
/// operator can mark a peer Tactical, an inbound form sharing that record's
/// stored base would pass the base-exists skip here yet still CREATE a new
/// record in `apply_observation` (the tactical anchor won't match) — an
/// unlimited inbound create. No path before Tactical assignment can produce
/// that state; the task that lands it must tighten this probe to mirror the
/// store's routing (tactical → exact presented match).
pub fn record_peer_observation(
    store: &Mutex<crate::peers::store::PeersStore>,
    limiter: &Mutex<crate::peers::limiter::InboundCreateLimiter>,
    obs: PeerObservation,
) -> crate::peers::store::ApplyEffect {
    use crate::peers::store::ApplyEffect;
    if classify(obs.phase) == Classified::NoRecord {
        return ApplyEffect::NoRecord;
    }
    // Rate-limit inbound CREATES only [R5-9]: existing-record updates and
    // outbound observations always pass.
    if obs.direction == Direction::Incoming {
        let base = crate::winlink::callsign::canonical_base(&obs.presented_target);
        let exists = store
            .lock()
            .map(|s| s.file().peers.iter().any(|p| p.canonical_base == base))
            .unwrap_or(false);
        if !exists {
            // Brand-new base: threshold on accepted vs failed.
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
                    target: "tuxlink::peers",
                    presented = %obs.presented_target,
                    quarantined_total = q,
                    "inbound peer auto-create rate-limited — quarantined (not added to roster)"
                );
                return ApplyEffect::NoRecord;
            }
        } else {
            // Base exists — the only inbound CREATE still possible is a held
            // `conflict` record on an operator-split base. Gate it through the
            // failed-path budget so a split-base flood cannot bypass the limiter
            // via the base-exists skip above (limiter amendment (b)(ii)).
            // `allow_conflict_creation` returns true (a no-op) for any
            // observation that would NOT create a conflict record, so a normal
            // update to an existing record is never limited. Lock order is
            // store→limiter here and nowhere reversed, so no deadlock.
            let allowed = match store.lock() {
                Ok(s) => limiter
                    .lock()
                    .map(|mut l| l.allow_conflict_creation(&s, &obs, std::time::Instant::now()))
                    .unwrap_or(true),
                Err(_) => true,
            };
            if !allowed {
                let q = limiter.lock().map(|l| l.quarantined()).unwrap_or(0);
                tracing::warn!(
                    target: "tuxlink::peers",
                    presented = %obs.presented_target,
                    quarantined_total = q,
                    "inbound conflict-record creation rate-limited — quarantined (not held)"
                );
                return ApplyEffect::NoRecord;
            }
        }
    }
    let now = chrono::Local::now().to_rfc3339();
    match store.lock() {
        Ok(mut s) => s.apply_observation(&obs, now).unwrap_or_else(|e| {
            tracing::warn!(target: "tuxlink::peers", "peer observation write failed: {e:?}");
            ApplyEffect::NoRecord
        }),
        Err(_) => ApplyEffect::NoRecord,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::peers::model::*;
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
    fn conflict_flood_through_the_central_entry_is_quarantined() {
        // Reviewer follow-up: end-to-end proof that record_peer_observation's
        // base-exists branch runs conflict CREATION through the failed-path
        // budget (limiter amendment (b)(ii)) — a split-base flood of distinct
        // unmatched presented forms is capped at failed_per_minute; everything
        // over budget returns NoRecord and never reaches the roster.
        use crate::peers::limiter::{InboundCreateLimiter, P2pLimitsConfig};
        use crate::peers::store::{ApplyEffect, PeersStore};

        let dir = tempfile::tempdir().unwrap();
        let store = Mutex::new(PeersStore::open(dir.path().join("peers.json")));
        let now = || "2026-07-10T12:00:00-07:00".to_string();
        {
            // Establish a split base: two twins on W6ABC, then split off -9.
            let mut s = store.lock().unwrap();
            s.apply_observation(
                &obs_for("W6ABC-7", Direction::Outgoing, ObservationPhase::B2fOk),
                now(),
            )
            .unwrap();
            s.apply_observation(
                &obs_for("W6ABC-9", Direction::Outgoing, ObservationPhase::B2fOk),
                now(),
            )
            .unwrap();
            let id = s.file().peers[0].id.clone();
            s.split(&id, vec!["W6ABC-9".to_string()], now()).unwrap();
        }
        let limiter = Mutex::new(InboundCreateLimiter::new(P2pLimitsConfig::default()));

        let (mut held, mut quarantined) = (0u32, 0u32);
        for i in 0..40u32 {
            // Distinct unmatched presented forms — each WOULD create a new
            // conflict record on the split base. (The test flood completes in
            // far under the limiter's 60s failed-path window.)
            let o = obs_for(
                &format!("W6ABC/Q{i}"),
                Direction::Incoming,
                ObservationPhase::Accepted,
            );
            match record_peer_observation(&store, &limiter, o) {
                ApplyEffect::ConflictHeld => held += 1,
                ApplyEffect::NoRecord => quarantined += 1,
                other => panic!("unexpected effect {other:?}"),
            }
        }
        assert_eq!(held, 10, "default failed_per_minute = 10 gates conflict creation");
        assert_eq!(quarantined, 30, "everything over budget is quarantined");
        let s = store.lock().unwrap();
        assert_eq!(
            s.file().peers.iter().filter(|p| p.conflict).count(),
            10,
            "the roster holds exactly the allowed budget — nothing beyond it"
        );
        assert_eq!(limiter.lock().unwrap().quarantined(), 30);
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
    #[serial_test::serial]
    fn global_sink_install_read_and_fire_roundtrip() {
        // The process-global sink (Task 13): install → observation_sink() returns
        // it → an ObservationGuard armed with it fires into the captured buffer on
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
}
