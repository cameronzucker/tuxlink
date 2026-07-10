//! Inbound auto-create rate limiter [R2-S6][R5-9]. Gates CREATION of
//! auto records from inbound observations only — outbound dials and
//! existing-record updates are never limited. In-memory; the quarantine
//! counter is never persisted to the roster.
//!
//! Accepted, authorized exchanges get the high per-hour threshold (a real
//! net/field-day must not lose roster observations); unauthorized/failed
//! bursts get the low per-minute threshold. Over-threshold events increment
//! a bounded in-memory quarantine counter; the caller is responsible for
//! logging visibly (`tracing::warn!` + session log) at the record site —
//! this module has no logging side effect of its own.
//!
//! Task 9 lands the limiter + its config + the quarantine counter. The
//! reject→limiter hook at allowlist-reject sites is Task 16's (a
//! process-global `record_inbound_reject`); this module does not call it.
//! What Task 9 DOES wire (amendment (b)(ii)): conflict-record CREATION
//! (Task 8's `PeersStore::would_create_conflict_record` seam) runs through
//! the failed path here before `PeersStore::apply_observation`, because a
//! split base always "exists" — the base-exists limiter skip elsewhere
//! would otherwise let conflict creation bypass rate-limiting entirely.

use crate::peers::model::ChannelTransport;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Config for [`InboundCreateLimiter`] (spec §2 caps [R5-9]). Additive
/// section on [`crate::config::Config`] — see `p2p_limits` there.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct P2pLimitsConfig {
    /// Accepted, authorized inbound exchanges (allowlist-passed,
    /// B2F-completed): high threshold — a real net must never lose
    /// roster observations.
    #[serde(default = "default_accepted_per_hour")]
    pub accepted_per_hour: u32,
    /// Unauthorized / failed / handshake-abandoned bursts: low threshold.
    #[serde(default = "default_failed_per_minute")]
    pub failed_per_minute: u32,
}

fn default_accepted_per_hour() -> u32 {
    100
}

fn default_failed_per_minute() -> u32 {
    10
}

impl Default for P2pLimitsConfig {
    fn default() -> Self {
        Self {
            accepted_per_hour: default_accepted_per_hour(),
            failed_per_minute: default_failed_per_minute(),
        }
    }
}

impl P2pLimitsConfig {
    /// Byte-for-byte equivalent to the default — used by `Config`'s
    /// `#[serde(skip_serializing_if)]` so an unconfigured operator's config
    /// file stays byte-identical to its pre-`p2p_limits` shape (mirrors the
    /// `ElmerConfig::is_default` pattern in `config.rs`), keeping this
    /// addition off the `CONFIG_SCHEMA_VERSION`-tracked always-serialized
    /// field set.
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// Bounded so a sustained flood cannot grow this counter without limit; the
/// exact cap value has no semantic meaning beyond "large enough to never
/// matter operationally, small enough to never overflow or be mistaken for
/// unbounded growth."
const QUARANTINE_COUNTER_CAP: u32 = 100_000;

/// Gates CREATION of auto peer records from inbound observations. Construct
/// once per process (or per test) via [`InboundCreateLimiter::new`]; state is
/// in-memory only.
pub struct InboundCreateLimiter {
    cfg: P2pLimitsConfig,
    accepted: HashMap<ChannelTransport, Vec<Instant>>,
    failed: HashMap<ChannelTransport, Vec<Instant>>,
    quarantined: u32,
}

impl InboundCreateLimiter {
    pub fn new(cfg: P2pLimitsConfig) -> Self {
        Self {
            cfg,
            accepted: HashMap::new(),
            failed: HashMap::new(),
            quarantined: 0,
        }
    }

    /// True = the auto-create may proceed. False = quarantined (count it,
    /// log visibly at the call site, do NOT write the roster). Buckets are
    /// per-`transport` and per-`accepted` — a burst on one transport or one
    /// bucket never exhausts another's budget.
    pub fn allow(&mut self, transport: ChannelTransport, accepted: bool, now: Instant) -> bool {
        let (bucket_map, window, max) = if accepted {
            (&mut self.accepted, Duration::from_secs(3600), self.cfg.accepted_per_hour)
        } else {
            (&mut self.failed, Duration::from_secs(60), self.cfg.failed_per_minute)
        };
        let bucket = bucket_map.entry(transport).or_default();
        bucket.retain(|t| now.duration_since(*t) < window);
        if (bucket.len() as u32) < max {
            bucket.push(now);
            true
        } else {
            self.quarantined = self.quarantined.saturating_add(1).min(QUARANTINE_COUNTER_CAP);
            false
        }
    }

    /// Gate conflict-record CREATION through the failed-path limiter
    /// (amendment (b)(ii)). Call BEFORE `PeersStore::apply_observation`
    /// with the SAME observation. `store.would_create_conflict_record(obs)`
    /// is the seam (Task 8) that makes "would this specific observation
    /// create a NEW conflict row" decidable — a split base always "exists",
    /// so without this check conflict creation would silently bypass rate
    /// limiting entirely.
    ///
    /// Returns `true` when the caller may proceed to `apply_observation`
    /// (either the observation is not a conflict-creating one at all, or it
    /// is and the failed-path budget allowed it). Returns `false` when the
    /// observation is quarantined: the caller must NOT call
    /// `apply_observation` for it.
    pub fn allow_conflict_creation(
        &mut self,
        store: &crate::peers::store::PeersStore,
        obs: &crate::peers::recorder::PeerObservation,
        now: Instant,
    ) -> bool {
        if !store.would_create_conflict_record(obs) {
            return true;
        }
        self.allow(transport_for_observation(obs), false, now)
    }

    /// Count of over-threshold events since construction, saturating at
    /// [`QUARANTINE_COUNTER_CAP`]. Never persisted to the roster.
    pub fn quarantined(&self) -> u32 {
        self.quarantined
    }
}

/// The limiter's per-transport bucket key for an observation. `Telnet` has
/// no `ChannelTransport` variant of its own, so telnet-sourced
/// conflict-creation attempts share the `Unknown` bucket — a real RF
/// transport never writes to that bucket, so it does not steal budget from
/// any RF transport's own bucket.
fn transport_for_observation(obs: &crate::peers::recorder::PeerObservation) -> ChannelTransport {
    match &obs.path {
        crate::peers::recorder::ObservedPath::Rf { transport, .. } => *transport,
        crate::peers::recorder::ObservedPath::Telnet { .. } => ChannelTransport::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::peers::model::{ChannelBandwidth, ChannelTransport, Direction};
    use crate::peers::recorder::{ObservationPhase, ObservedPath, PeerObservation};
    use crate::peers::store::PeersStore;
    use std::time::{Duration, Instant};

    fn limiter() -> InboundCreateLimiter {
        InboundCreateLimiter::new(P2pLimitsConfig::default())
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

    #[test]
    fn a_busy_field_day_is_not_quarantined() {
        // [R5-9]: 50 distinct accepted inbound exchanges in an hour is a
        // real net; all must pass at the default threshold of 100/hr.
        let mut l = limiter();
        let t0 = Instant::now();
        for i in 0..50 {
            assert!(l.allow(ChannelTransport::VaraHf, true, t0 + Duration::from_secs(i * 60)));
        }
        assert_eq!(l.quarantined(), 0);
    }

    #[test]
    fn a_failed_handshake_burst_hits_the_low_threshold() {
        let mut l = limiter();
        let t0 = Instant::now();
        let mut allowed = 0;
        for i in 0..40 {
            if l.allow(ChannelTransport::VaraHf, false, t0 + Duration::from_millis(i * 100)) {
                allowed += 1;
            }
        }
        assert_eq!(allowed, 10, "default failed_per_minute = 10");
        assert_eq!(l.quarantined(), 30);
    }

    #[test]
    fn thresholds_are_per_transport() {
        let mut l = limiter();
        let t0 = Instant::now();
        for i in 0..10 {
            assert!(l.allow(ChannelTransport::VaraHf, false, t0 + Duration::from_millis(i)));
        }
        assert!(!l.allow(ChannelTransport::VaraHf, false, t0 + Duration::from_millis(11)));
        // Packet has its own bucket — not exhausted by the VARA burst.
        assert!(l.allow(ChannelTransport::Packet, false, t0 + Duration::from_millis(12)));
    }

    #[test]
    fn quarantine_counter_is_bounded() {
        let mut l = limiter();
        let t0 = Instant::now();
        for i in 0..200_000u64 {
            let _ = l.allow(ChannelTransport::VaraHf, false, t0 + Duration::from_millis(i));
        }
        assert!(l.quarantined() <= 100_000, "counter saturates; no unbounded growth");
    }

    #[test]
    fn conflict_creation_bursts_are_quarantined_via_the_failed_path() {
        // Amendment (b)(ii): a burst of NEW conflict-record creations on a
        // split base runs through the limiter's failed path — same budget
        // as any other failed/unauthorized inbound burst — so a flood of
        // distinct unmatched presented forms cannot bypass rate limiting
        // just because the base itself already "exists".
        let dir = tempfile::tempdir().unwrap();
        let mut store = PeersStore::open(dir.path().join("peers.json"));
        let now_str = || "2026-07-10T12:00:00-07:00".to_string();

        // Establish a split base: two twins on W6ABC, then split off -9.
        store
            .apply_observation(&rf_obs("W6ABC-7", Direction::Outgoing, ObservationPhase::B2fOk), now_str())
            .unwrap();
        store
            .apply_observation(&rf_obs("W6ABC-9", Direction::Outgoing, ObservationPhase::B2fOk), now_str())
            .unwrap();
        let id = store.file().peers[0].id.clone();
        store.split(&id, vec!["W6ABC-9".to_string()], now_str()).unwrap();

        let mut l = limiter();
        let t0 = Instant::now();
        let mut allowed = 0;
        for i in 0..40u32 {
            // A distinct unmatched presented form each time — every one of
            // these WOULD create a new conflict record on the split base.
            let obs = rf_obs(
                &format!("W6ABC/Q{i}"),
                Direction::Incoming,
                ObservationPhase::Accepted,
            );
            let now = t0 + Duration::from_millis(u64::from(i) * 100);
            if l.allow_conflict_creation(&store, &obs, now) {
                allowed += 1;
                // Only a caller that got `true` may proceed to apply it —
                // a quarantined observation never reaches the roster.
                store.apply_observation(&obs, now_str()).unwrap();
            }
        }
        assert_eq!(allowed, 10, "default failed_per_minute = 10 gates conflict creation too");
        assert_eq!(l.quarantined(), 30, "the other 30 conflict creations were quarantined");
    }

    #[test]
    fn allow_conflict_creation_never_gates_non_conflict_observations() {
        // A plain new-base observation (no split, no twin) is not a
        // conflict creation at all — allow_conflict_creation must not
        // consume failed-path budget for it, regardless of volume.
        let dir = tempfile::tempdir().unwrap();
        let store = PeersStore::open(dir.path().join("peers.json"));
        let mut l = limiter();
        let t0 = Instant::now();
        for i in 0..40u32 {
            let obs = rf_obs("W6ABC-7", Direction::Incoming, ObservationPhase::Accepted);
            assert!(l.allow_conflict_creation(&store, &obs, t0 + Duration::from_millis(u64::from(i))));
        }
        assert_eq!(l.quarantined(), 0, "non-conflict observations never touch the failed-path budget");
    }
}
