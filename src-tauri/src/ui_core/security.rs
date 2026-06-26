//! Egress authorization for the MCP server's agent caller.
//!
//! Today, an operator fires a connection by clicking Connect / Send-Receive /
//! Start; that click IS the authorization (the Part 97 consent — the RF panels
//! never auto-connect). This module changes none of that: a GUI-initiated call
//! passes [`EgressAuthority::Operator`] and is always allowed.
//!
//! The MCP server adds a NEW caller that can invoke the same connect operation
//! WITHOUT a button click. That path passes [`EgressAuthority::Agent`] and is
//! allowed only while the operator has ARMED send authority AND the session is
//! NOT tainted by untrusted message content. `decide` is the pure heart;
//! [`EgressGuard`] holds the live state.

use std::sync::Mutex;
use thiserror::Error;

/// Who is requesting an egress (anything that leaves the box: RF emit, internet
/// send, outbox-flush).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EgressAuthority {
    /// The human control operator acting directly via the GUI. Always allowed.
    Operator,
    /// An automated agent via the MCP server. Gated behind armed + un-tainted.
    Agent,
}

/// Why an egress was refused for an [`EgressAuthority::Agent`] caller.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum EgressDenied {
    #[error("send authority is not armed")]
    NotArmed,
    #[error("send authority expired {0}s ago; re-arm to continue")]
    Expired(u64),
    #[error("session is tainted by untrusted message content; egress blocked")]
    Tainted,
}

/// Pure authorization decision. `armed_until` is a unix-seconds deadline (None =
/// disarmed); `now` is the current unix seconds. Taint takes precedence over the
/// armed check, so a poisoned session is blocked even while armed.
pub fn decide(
    armed_until: Option<u64>,
    tainted: bool,
    authority: EgressAuthority,
    now: u64,
) -> Result<(), EgressDenied> {
    if authority == EgressAuthority::Operator {
        return Ok(());
    }
    // Agent: taint is checked first (a poisoned session never egresses).
    if tainted {
        return Err(EgressDenied::Tainted);
    }
    match armed_until {
        None => Err(EgressDenied::NotArmed),
        Some(deadline) if now < deadline => Ok(()),
        Some(deadline) => Err(EgressDenied::Expired(now.saturating_sub(deadline))),
    }
}

/// Live armed-grant + taint state for egress authorization. Registered as Tauri
/// managed state. `now_unix` is injectable so tests pin deterministic deadlines.
pub struct EgressGuard {
    inner: Mutex<EgressGuardInner>,
    now_unix: fn() -> u64,
}

struct EgressGuardInner {
    /// Unix-seconds deadline; `None` when disarmed.
    armed_until: Option<u64>,
    /// Set when untrusted content is read; cleared only by an explicit reset.
    tainted: bool,
}

fn real_now_unix() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

impl EgressGuard {
    pub fn new() -> Self {
        Self::with_clock(real_now_unix)
    }

    pub fn with_clock(now_unix: fn() -> u64) -> Self {
        Self {
            inner: Mutex::new(EgressGuardInner { armed_until: None, tainted: false }),
            now_unix,
        }
    }

    /// Arm send authority for `duration_secs` from now. Returns the deadline.
    pub fn arm(&self, duration_secs: u64) -> u64 {
        let deadline = (self.now_unix)().saturating_add(duration_secs);
        self.inner.lock().unwrap().armed_until = Some(deadline);
        deadline
    }

    pub fn disarm(&self) {
        self.inner.lock().unwrap().armed_until = None;
    }

    pub fn taint(&self) {
        self.inner.lock().unwrap().tainted = true;
    }

    pub fn clear_taint(&self) {
        self.inner.lock().unwrap().tainted = false;
    }

    pub fn is_tainted(&self) -> bool {
        self.inner.lock().unwrap().tainted
    }

    /// Seconds remaining on the armed grant; 0 if disarmed or expired.
    pub fn armed_remaining(&self) -> u64 {
        let g = self.inner.lock().unwrap();
        match g.armed_until {
            Some(deadline) => deadline.saturating_sub((self.now_unix)()),
            None => 0,
        }
    }

    /// THE GATE. Authorize an egress for `authority` against the live state.
    pub fn authorize(&self, authority: EgressAuthority) -> Result<(), EgressDenied> {
        let g = self.inner.lock().unwrap();
        decide(g.armed_until, g.tainted, authority, (self.now_unix)())
    }
}

impl Default for EgressGuard {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operator_is_always_allowed_regardless_of_state() {
        // Disarmed, tainted, expired — none matter for the present human.
        assert!(decide(None, true, EgressAuthority::Operator, 1000).is_ok());
        assert!(decide(Some(1), true, EgressAuthority::Operator, 1000).is_ok());
    }

    #[test]
    fn agent_unarmed_is_not_armed() {
        assert_eq!(
            decide(None, false, EgressAuthority::Agent, 1000),
            Err(EgressDenied::NotArmed)
        );
    }

    #[test]
    fn agent_armed_and_untainted_before_deadline_is_allowed() {
        assert!(decide(Some(1030), false, EgressAuthority::Agent, 1000).is_ok());
    }

    #[test]
    fn agent_armed_at_exact_deadline_is_expired() {
        // Deadline is exclusive: now == deadline means expired (0s ago).
        assert_eq!(
            decide(Some(1000), false, EgressAuthority::Agent, 1000),
            Err(EgressDenied::Expired(0))
        );
    }

    #[test]
    fn agent_armed_past_deadline_reports_seconds_since_expiry() {
        assert_eq!(
            decide(Some(1000), false, EgressAuthority::Agent, 1075),
            Err(EgressDenied::Expired(75))
        );
    }

    #[test]
    fn agent_tainted_is_blocked_even_when_armed() {
        assert_eq!(
            decide(Some(9999), true, EgressAuthority::Agent, 1000),
            Err(EgressDenied::Tainted)
        );
    }

    #[test]
    fn agent_tainted_takes_precedence_over_unarmed() {
        // Both tainted and unarmed → Tainted is reported (the security-salient one).
        assert_eq!(
            decide(None, true, EgressAuthority::Agent, 1000),
            Err(EgressDenied::Tainted)
        );
    }

    // Deterministic clock fixed at 1000 for state-transition tests.
    fn fixed_1000() -> u64 { 1000 }

    #[test]
    fn arm_sets_a_deadline_and_authorizes_agent() {
        let g = EgressGuard::with_clock(fixed_1000);
        let deadline = g.arm(30);
        assert_eq!(deadline, 1030);
        assert!(g.authorize(EgressAuthority::Agent).is_ok());
        assert_eq!(g.armed_remaining(), 30);
    }

    #[test]
    fn disarm_revokes() {
        let g = EgressGuard::with_clock(fixed_1000);
        g.arm(30);
        g.disarm();
        assert_eq!(g.authorize(EgressAuthority::Agent), Err(EgressDenied::NotArmed));
        assert_eq!(g.armed_remaining(), 0);
    }

    #[test]
    fn taint_blocks_agent_and_survives_arming() {
        let g = EgressGuard::with_clock(fixed_1000);
        g.taint();
        g.arm(30); // arming must NOT clear taint (closes the read->arm bypass)
        assert!(g.is_tainted());
        assert_eq!(g.authorize(EgressAuthority::Agent), Err(EgressDenied::Tainted));
    }

    #[test]
    fn clear_taint_re_enables_after_explicit_reset() {
        let g = EgressGuard::with_clock(fixed_1000);
        g.taint();
        g.arm(30);
        g.clear_taint(); // explicit session reset, distinct from arm
        assert!(!g.is_tainted());
        assert!(g.authorize(EgressAuthority::Agent).is_ok());
    }

    #[test]
    fn operator_authorizes_even_when_disarmed_and_tainted() {
        let g = EgressGuard::with_clock(fixed_1000);
        g.taint();
        assert!(g.authorize(EgressAuthority::Operator).is_ok());
    }
}
