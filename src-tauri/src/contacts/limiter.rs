//! Inbound auto-create rate limiter [R2-S6][R5-9]. Gates CREATION of
//! `ContactTier::Unconfirmed` records from inbound observations only —
//! outbound dials and existing-record updates are never limited, and
//! `Confirmed` contacts are never auto-created at all (spec §AMENDMENT
//! pt. 8). In-memory; the quarantine counter is never persisted.
//!
//! Moved verbatim from the deleted `peers/limiter.rs` (contacts-superset
//! pivot 2026-07-10/11), minus the conflict-record gate — conflict records
//! died with the identity-merge machinery.
//!
//! Accepted, authorized exchanges get the high per-hour threshold (a real
//! net/field-day must not lose roster observations); unauthorized/failed
//! bursts get the low per-minute threshold. Over-threshold events increment
//! a bounded in-memory quarantine counter; the caller is responsible for
//! logging visibly (`tracing::warn!` + session log) at the record site —
//! this module has no logging side effect of its own.

use crate::contacts::reachability::ChannelTransport;
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

/// Gates CREATION of unconfirmed contact records from inbound observations.
/// Construct once per process (or per test) via [`InboundCreateLimiter::new`];
/// state is in-memory only.
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

    /// Count of over-threshold events since construction, saturating at
    /// [`QUARANTINE_COUNTER_CAP`]. Never persisted to the roster.
    pub fn quarantined(&self) -> u32 {
        self.quarantined
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn limiter() -> InboundCreateLimiter {
        InboundCreateLimiter::new(P2pLimitsConfig::default())
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
}
