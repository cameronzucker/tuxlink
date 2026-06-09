//! `tiles::breaker` — the source-level circuit breaker (§8.5).
//!
//! ## Why a breaker
//!
//! Each tile request that misses the cache spawns an SSRF-guarded upstream
//! fetch bounded by a 5 s timeout. When a LAN source goes down (radio off,
//! laptop closed, network partition) every visible tile would otherwise issue
//! its own fetch and wait the full timeout before failing — a per-tile timeout
//! storm that hangs the view for seconds at a time, repeatedly, as the operator
//! pans. The breaker collapses that storm: after K consecutive *host* failures
//! it trips `Degraded` and short-circuits subsequent fetches for a cooldown,
//! so the UI serves the bundled raster instead of waiting on a dead source. On
//! cooldown expiry it allows exactly ONE re-probe; success re-arms `Live`,
//! failure re-arms the cooldown.
//!
//! ## Failure classification (load-bearing — §8.5)
//!
//! Two outcome classes are distinct and MUST NOT be conflated:
//!
//! - A per-tile **404 (NotFound)** above the source's raster-native zoom is a
//!   *coverage gap*, not a source-health signal: the source is live, this tile
//!   is simply absent. A 404 records [`Outcome::Coverage`] which DOES NOT
//!   increment the failure counter and never trips the breaker. (It marks the
//!   source `Partial` for the status surface.)
//! - A **host failure** — `HostDenied` / `Redirect` / `Status` / `Network` /
//!   timeout — is a source-health signal. It records [`Outcome::HostFailure`]
//!   which increments the counter; the K-th consecutive one trips the breaker.
//!
//! A success ([`Outcome::Success`]) resets the counter to 0, so an intermittent
//! source (fail, fail, succeed, fail, fail) never trips: only K *consecutive*
//! host failures do.
//!
//! ## Injected clock (testability)
//!
//! The breaker NEVER calls [`std::time::Instant::now`] itself. The caller owns
//! the clock and passes the current instant into every state-transition method
//! ([`record`], [`should_attempt`]). Production passes `Instant::now()`; tests
//! pass a hand-advanced [`Instant`] so cooldown expiry is deterministic with no
//! sleeping. This is the seam the plan flags for Task 9.1.

use std::time::{Duration, Instant};

/// Consecutive host-failures that trip the breaker. The 3rd consecutive host
/// failure flips `Live → Degraded`.
pub const FAILURE_THRESHOLD: u32 = 3;

/// How long the breaker stays degraded before allowing a single re-probe.
pub const COOLDOWN: Duration = Duration::from_secs(30);

/// The breaker's externally-visible health for the active source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BreakerHealth {
    /// Source is healthy: per-tile fetches proceed normally.
    Live,
    /// Source tripped the breaker: per-tile fetches are short-circuited until
    /// the cooldown expires (serve bundled instead of storming timeouts).
    Degraded,
}

/// The outcome of a single attempted tile fetch, fed back to the breaker so it
/// can advance its state. See the module docs for the 404-vs-host distinction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// A tile was fetched (or cache-served) successfully. Resets the counter.
    Success,
    /// A host-level failure (denied/redirect/status/network/timeout). Counts
    /// toward the trip threshold.
    HostFailure,
    /// A 404 above coverage — the source is live, the tile is absent. Does NOT
    /// count toward the trip threshold; marks the source partial.
    Coverage,
}

/// The internal phase the breaker is in. `Degraded` carries the instant the
/// cooldown was (re-)armed so [`should_attempt`] can decide when one re-probe
/// is due. `Probing` is the transient phase between "cooldown expired, one
/// probe authorized" and the probe's outcome arriving via [`record`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    /// Healthy. `failures` consecutive host-failures observed (0..K).
    Live { failures: u32 },
    /// Tripped at `since`; short-circuit until `since + COOLDOWN`.
    Degraded { since: Instant },
    /// One re-probe authorized after cooldown expiry; awaiting its outcome.
    Probing,
}

/// Source-level circuit breaker with an injected clock.
///
/// Construction does NO I/O and starts `Live` with a zero failure count — it
/// engages only as outcomes are recorded during serving, so a gatekeeper built
/// at startup performs no synchronous network probe.
#[derive(Debug)]
pub struct CircuitBreaker {
    phase: Phase,
}

impl Default for CircuitBreaker {
    fn default() -> Self {
        Self::new()
    }
}

impl CircuitBreaker {
    /// A fresh breaker: `Live`, zero failures, no I/O.
    pub fn new() -> Self {
        CircuitBreaker {
            phase: Phase::Live { failures: 0 },
        }
    }

    /// The externally-visible health AT `now`.
    ///
    /// `Live`/`Probing` report [`BreakerHealth::Live`]; `Degraded` reports
    /// [`BreakerHealth::Degraded`] while within the cooldown, and `Live` once
    /// the cooldown has elapsed (a re-probe is now due — the source is treated
    /// as live-pending-probe, and [`should_attempt`] will authorize the probe).
    pub fn health(&self, now: Instant) -> BreakerHealth {
        match self.phase {
            Phase::Live { .. } | Phase::Probing => BreakerHealth::Live,
            Phase::Degraded { since } => {
                if now.duration_since(since) >= COOLDOWN {
                    BreakerHealth::Live
                } else {
                    BreakerHealth::Degraded
                }
            }
        }
    }

    /// Should `serve_tile` issue a per-tile fetch right now?
    ///
    /// - `Live` → yes.
    /// - `Degraded` within cooldown → NO (short-circuit; serve bundled). This
    ///   is the storm-suppression: no per-tile network while cooling.
    /// - `Degraded` past cooldown → yes, exactly ONE re-probe is authorized;
    ///   the breaker transitions to `Probing` so a second concurrent caller
    ///   does NOT also probe (it sees `Probing` → short-circuits) until the
    ///   first probe's [`record`] resolves the phase.
    /// - `Probing` → NO (a probe is already in flight; don't pile on).
    pub fn should_attempt(&mut self, now: Instant) -> bool {
        match self.phase {
            Phase::Live { .. } => true,
            Phase::Probing => false,
            Phase::Degraded { since } => {
                if now.duration_since(since) >= COOLDOWN {
                    // Cooldown elapsed: authorize exactly one re-probe.
                    self.phase = Phase::Probing;
                    true
                } else {
                    false
                }
            }
        }
    }

    /// Feed back the outcome of an attempted fetch, advancing the state machine.
    ///
    /// - [`Outcome::Success`] → reset to `Live { failures: 0 }`.
    /// - [`Outcome::Coverage`] (404 above coverage) → NO state change to the
    ///   failure count (the source is live; this is a coverage gap). A coverage
    ///   result while `Probing` counts as the source being reachable → `Live`.
    /// - [`Outcome::HostFailure`]:
    ///   - from `Live`: increment; the K-th consecutive trips `Degraded` at
    ///     `now`.
    ///   - from `Probing`: the re-probe failed → re-arm `Degraded` at `now`.
    ///   - from `Degraded`: a stray late result while cooling — re-arm the
    ///     cooldown clock at `now` (defensive; should_attempt gates fetches).
    pub fn record(&mut self, outcome: Outcome, now: Instant) {
        match outcome {
            Outcome::Success => {
                self.phase = Phase::Live { failures: 0 };
            }
            Outcome::Coverage => match self.phase {
                // A reachable 404 during a re-probe proves the source is up.
                Phase::Probing => self.phase = Phase::Live { failures: 0 },
                // Otherwise a coverage gap does not touch source health.
                Phase::Live { .. } | Phase::Degraded { .. } => {}
            },
            Outcome::HostFailure => match self.phase {
                Phase::Live { failures } => {
                    let next = failures + 1;
                    if next >= FAILURE_THRESHOLD {
                        self.phase = Phase::Degraded { since: now };
                    } else {
                        self.phase = Phase::Live { failures: next };
                    }
                }
                Phase::Probing | Phase::Degraded { .. } => {
                    self.phase = Phase::Degraded { since: now };
                }
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t0() -> Instant {
        Instant::now()
    }

    #[test]
    fn fresh_breaker_is_live_and_attempts() {
        let mut b = CircuitBreaker::new();
        let now = t0();
        assert_eq!(b.health(now), BreakerHealth::Live);
        assert!(b.should_attempt(now), "a fresh breaker permits fetches");
    }

    #[test]
    fn three_consecutive_host_failures_trip_degraded() {
        let mut b = CircuitBreaker::new();
        let now = t0();
        b.record(Outcome::HostFailure, now);
        assert_eq!(b.health(now), BreakerHealth::Live, "1 failure: still live");
        b.record(Outcome::HostFailure, now);
        assert_eq!(b.health(now), BreakerHealth::Live, "2 failures: still live");
        b.record(Outcome::HostFailure, now);
        assert_eq!(b.health(now), BreakerHealth::Degraded, "3rd consecutive: tripped");
        // While cooling, no fetch is attempted (storm suppression).
        assert!(!b.should_attempt(now), "degraded+cooling short-circuits fetches");
    }

    #[test]
    fn coverage_404_does_not_increment_the_breaker() {
        let mut b = CircuitBreaker::new();
        let now = t0();
        // Even a flood of 404s never trips the breaker — they're coverage gaps.
        for _ in 0..10 {
            b.record(Outcome::Coverage, now);
        }
        assert_eq!(b.health(now), BreakerHealth::Live);
        assert!(b.should_attempt(now));
        // And a 404 interleaved with host failures does not advance the trip
        // count: 2 host-failures + a 404 + 2 host-failures = still below K from
        // EITHER run (the success-less 404 leaves the count untouched, so the
        // run continues — but note 404 does not RESET it either).
        let mut b = CircuitBreaker::new();
        b.record(Outcome::HostFailure, now);
        b.record(Outcome::HostFailure, now);
        b.record(Outcome::Coverage, now); // no increment, no reset
        assert_eq!(b.health(now), BreakerHealth::Live, "2 host-failures + 404: still live");
    }

    #[test]
    fn success_resets_the_consecutive_counter() {
        // 2 host-failures + 1 success + 2 host-failures must NOT trip: the
        // success resets the consecutive run, so neither run reaches K=3.
        let mut b = CircuitBreaker::new();
        let now = t0();
        b.record(Outcome::HostFailure, now);
        b.record(Outcome::HostFailure, now);
        b.record(Outcome::Success, now);
        b.record(Outcome::HostFailure, now);
        b.record(Outcome::HostFailure, now);
        assert_eq!(
            b.health(now),
            BreakerHealth::Live,
            "success between runs prevents a trip"
        );
        assert!(b.should_attempt(now));
    }

    #[test]
    fn within_cooldown_stays_degraded_and_short_circuits() {
        let mut b = CircuitBreaker::new();
        let now = t0();
        for _ in 0..FAILURE_THRESHOLD {
            b.record(Outcome::HostFailure, now);
        }
        assert_eq!(b.health(now), BreakerHealth::Degraded);
        // 29 s in (< 30 s cooldown): still degraded, still short-circuiting.
        let almost = now + Duration::from_secs(29);
        assert_eq!(b.health(almost), BreakerHealth::Degraded);
        assert!(!b.should_attempt(almost), "still cooling: no fetch");
    }

    #[test]
    fn after_cooldown_exactly_one_reprobe_is_authorized() {
        let mut b = CircuitBreaker::new();
        let now = t0();
        for _ in 0..FAILURE_THRESHOLD {
            b.record(Outcome::HostFailure, now);
        }
        let after = now + COOLDOWN;
        // First caller after cooldown: probe authorized.
        assert!(b.should_attempt(after), "cooldown elapsed: one probe authorized");
        // A SECOND caller before the probe resolves must NOT also probe.
        assert!(
            !b.should_attempt(after),
            "only one re-probe in flight at a time"
        );
    }

    #[test]
    fn reprobe_success_resets_to_live() {
        let mut b = CircuitBreaker::new();
        let now = t0();
        for _ in 0..FAILURE_THRESHOLD {
            b.record(Outcome::HostFailure, now);
        }
        let after = now + COOLDOWN;
        assert!(b.should_attempt(after));
        b.record(Outcome::Success, after);
        assert_eq!(b.health(after), BreakerHealth::Live, "successful re-probe → live");
        assert!(b.should_attempt(after), "live again: fetches resume");
    }

    #[test]
    fn reprobe_failure_rearms_the_cooldown() {
        let mut b = CircuitBreaker::new();
        let now = t0();
        for _ in 0..FAILURE_THRESHOLD {
            b.record(Outcome::HostFailure, now);
        }
        let after = now + COOLDOWN;
        assert!(b.should_attempt(after), "first probe authorized");
        b.record(Outcome::HostFailure, after); // probe failed
        assert_eq!(b.health(after), BreakerHealth::Degraded, "failed re-probe → re-armed");
        // The cooldown clock restarted at `after`: not yet re-probe-able.
        assert!(
            !b.should_attempt(after + Duration::from_secs(29)),
            "re-armed cooldown still cooling"
        );
        // And a full cooldown later, one probe is authorized again.
        assert!(b.should_attempt(after + COOLDOWN), "re-armed cooldown elapsed");
    }

    #[test]
    fn reprobe_coverage_404_proves_source_is_up() {
        // A 404 during the re-probe means the source ANSWERED — it's reachable,
        // just missing that tile. Treat as recovery, not a re-arm.
        let mut b = CircuitBreaker::new();
        let now = t0();
        for _ in 0..FAILURE_THRESHOLD {
            b.record(Outcome::HostFailure, now);
        }
        let after = now + COOLDOWN;
        assert!(b.should_attempt(after));
        b.record(Outcome::Coverage, after);
        assert_eq!(b.health(after), BreakerHealth::Live, "reachable 404 → recovered");
    }
}
