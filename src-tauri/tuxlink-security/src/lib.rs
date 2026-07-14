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
//!
//! This is a standalone crate (extracted in MCP phase 3.1) so both the Tauri
//! monolith and the standalone tier-2 testserver depend on the SAME real
//! authority without pulling in the Tauri app. The monolith re-exports it from
//! `crate::ui_core::security`.

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

/// Why the session is tainted — names the OPERATION that read untrusted content,
/// NEVER the content itself. Content-free by construction: the enum carries no
/// `String`/data payload, so a taint reason can never re-inject or leak the
/// untrusted material that caused the taint (the taint sites hold attacker-
/// controlled DTOs in scope — deriving a reason from them would be a leak channel
/// through the always-available `server_info` surface). Surfaced to the agent via
/// `server_info` so it can explain the transmit lock and its remedy.
///
/// `#[non_exhaustive]`: new taint sources may be added without breaking matchers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum TaintReason {
    /// `mailbox_list` — untrusted senders/subjects.
    MailboxList,
    /// `message_read` — untrusted message body.
    MessageRead,
    /// message search — untrusted results.
    SearchResults,
    /// `session_log_snapshot` — may contain untrusted wire content.
    SessionLog,
    /// `routines_journal_get` — a run's step outputs/errors carry verbatim
    /// wire content (gateway/CMS/VARA text), same as `session_log_snapshot`.
    RoutinesJournal,
}

impl TaintReason {
    /// Stable snake_case token for wire / UI / agent-facing surfaces. Kept here
    /// (not a serde derive) so the security crate stays dependency-free; the
    /// mcp-core DTO serializes this token.
    pub fn as_str(&self) -> &'static str {
        match self {
            TaintReason::MailboxList => "mailbox_list",
            TaintReason::MessageRead => "message_read",
            TaintReason::SearchResults => "search_results",
            TaintReason::SessionLog => "session_log",
            TaintReason::RoutinesJournal => "routines_journal",
        }
    }
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
    /// Why the session was tainted (the FIRST taint's operation; monotonic within
    /// a run). `None` when un-tainted. Content-free ([`TaintReason`]).
    taint_reason: Option<TaintReason>,
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
            inner: Mutex::new(EgressGuardInner {
                armed_until: None,
                tainted: false,
                taint_reason: None,
            }),
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

    /// Mark the session tainted, recording WHY (the operation that read untrusted
    /// content). First taint wins — the reason is monotonic within a run, so a
    /// later read cannot overwrite the original cause. Idempotent on `tainted`.
    pub fn taint(&self, reason: TaintReason) {
        let mut g = self.inner.lock().unwrap();
        if !g.tainted {
            g.taint_reason = Some(reason);
        }
        g.tainted = true;
    }

    pub fn clear_taint(&self) {
        let mut g = self.inner.lock().unwrap();
        g.tainted = false;
        g.taint_reason = None;
    }

    /// Atomically clear taint AND set a fresh arm deadline in one locked act.
    /// The ONLY sanctioned re-enable-after-taint path; the caller (egress_rearm)
    /// pairs it with dropping the tainted conversation. Clearing taint without
    /// replacing the deadline would reopen egress against a stale-but-live arm.
    pub fn quarantine_and_rearm(&self, duration_secs: u64) -> u64 {
        let now = (self.now_unix)();
        let deadline = now.saturating_add(duration_secs);
        let mut g = self.inner.lock().unwrap();
        g.tainted = false;
        g.taint_reason = None;
        g.armed_until = Some(deadline);
        deadline
    }

    pub fn is_tainted(&self) -> bool {
        self.inner.lock().unwrap().tainted
    }

    /// The recorded taint cause (content-free), or `None` when un-tainted.
    pub fn taint_reason(&self) -> Option<TaintReason> {
        self.inner.lock().unwrap().taint_reason
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
    ///
    /// Fail-closed on a poisoned mutex: if a prior holder panicked while
    /// mutating the guard, we cannot trust the armed/taint state, so an
    /// [`EgressAuthority::Agent`] caller is DENIED with [`EgressDenied::Tainted`]
    /// (poison ≡ untrusted state ≡ deny) rather than panicking. The
    /// [`EgressAuthority::Operator`] path is answered BEFORE the lock — an
    /// operator's button click is unconditionally allowed and never reads
    /// armed/taint — so a poisoned guard cannot strand the human operator.
    pub fn authorize(&self, authority: EgressAuthority) -> Result<(), EgressDenied> {
        // Operator is always allowed and needs no state; answer before locking
        // so a poisoned lock never blocks the present human.
        if authority == EgressAuthority::Operator {
            return Ok(());
        }
        // Agent: read the live state. A poisoned lock means a prior holder
        // panicked mid-mutation; treat the session as untrusted → DENIED.
        match self.inner.lock() {
            Ok(g) => decide(g.armed_until, g.tainted, authority, (self.now_unix)()),
            Err(_poisoned) => Err(EgressDenied::Tainted),
        }
    }
}

/// One audit record describing an egress authorization decision, handed to the
/// [`guarded_egress`] caller's audit sink. Borrows the op label so the sink can
/// log it without an allocation; `reason` is owned (the denial's `Display`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EgressAudit<'a> {
    /// A short label for the operation being gated (e.g. `"cms_connect"`).
    pub op: &'a str,
    /// Who requested the egress.
    pub authority: EgressAuthority,
    /// True when the authorization passed and the op was about to run.
    pub allowed: bool,
    /// On denial, the human-readable reason (the `EgressDenied` `Display`);
    /// `None` when allowed.
    pub reason: Option<String>,
}

/// Gate an arbitrary async egress operation through the [`EgressGuard`].
///
/// This is the ONE place egress capability and authorization meet: it calls
/// [`EgressGuard::authorize`] for `authority`, audits the decision, and only
/// runs `op` when authorization passed. On denial `op` is NEVER constructed nor
/// awaited — the closure is consumed only on the allowed path — so a gated
/// capability cannot fire while disarmed / expired / tainted / poisoned.
///
/// `audit` is invoked exactly once per call (allowed or denied) before the
/// result is returned, so callers get a complete decision trail.
pub async fn guarded_egress<T, F, Fut>(
    guard: &EgressGuard,
    authority: EgressAuthority,
    op_label: &str,
    audit: &(dyn Fn(EgressAudit<'_>) + Send + Sync),
    op: F,
) -> Result<T, EgressDenied>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = T>,
{
    match guard.authorize(authority) {
        Ok(()) => {
            audit(EgressAudit {
                op: op_label,
                authority,
                allowed: true,
                reason: None,
            });
            Ok(op().await)
        }
        Err(denied) => {
            audit(EgressAudit {
                op: op_label,
                authority,
                allowed: false,
                reason: Some(denied.to_string()),
            });
            Err(denied)
        }
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
        g.taint(TaintReason::MessageRead);
        g.arm(30); // arming must NOT clear taint (closes the read->arm bypass)
        assert!(g.is_tainted());
        assert_eq!(g.authorize(EgressAuthority::Agent), Err(EgressDenied::Tainted));
    }

    #[test]
    fn clear_taint_re_enables_after_explicit_reset() {
        let g = EgressGuard::with_clock(fixed_1000);
        g.taint(TaintReason::MessageRead);
        g.arm(30);
        g.clear_taint(); // explicit session reset, distinct from arm
        assert!(!g.is_tainted());
        assert!(g.authorize(EgressAuthority::Agent).is_ok());
    }

    #[test]
    fn operator_authorizes_even_when_disarmed_and_tainted() {
        let g = EgressGuard::with_clock(fixed_1000);
        g.taint(TaintReason::MessageRead);
        assert!(g.authorize(EgressAuthority::Operator).is_ok());
    }

    // --- STEP 1: poison fail-closed ---

    /// Poison the guard's mutex by panicking while holding the lock, then assert
    /// the fail-closed posture: Agent is DENIED (Tainted), Operator still Ok.
    #[test]
    fn poisoned_guard_denies_agent_but_allows_operator() {
        use std::panic::{catch_unwind, AssertUnwindSafe};
        use std::sync::Arc;

        let g = Arc::new(EgressGuard::with_clock(fixed_1000));
        // Arm it so that, absent poison, an Agent WOULD be allowed — proving the
        // denial is the poison, not an unarmed state.
        g.arm(30);
        assert!(g.authorize(EgressAuthority::Agent).is_ok());

        // Panic while holding the inner lock → poisons the Mutex.
        let g2 = Arc::clone(&g);
        let _ = catch_unwind(AssertUnwindSafe(|| {
            let _held = g2.inner.lock().unwrap();
            panic!("poison the mutex on purpose");
        }));

        // Agent is now denied (state untrusted), reported as Tainted.
        assert_eq!(
            g.authorize(EgressAuthority::Agent),
            Err(EgressDenied::Tainted),
            "a poisoned guard must deny the Agent fail-closed"
        );
        // Operator is answered before the lock → still allowed.
        assert!(
            g.authorize(EgressAuthority::Operator).is_ok(),
            "a poisoned guard must NOT strand the human operator"
        );
    }

    // --- STEP 2: guarded_egress ---

    // A minimal current-thread runtime via #[tokio::test] (rt + macros features).

    /// Outcome of a [`run_guarded`] call: the gate result, whether the op ran,
    /// and the captured (allowed, reason) audit records.
    struct GuardedOutcome {
        res: Result<u32, EgressDenied>,
        ran: bool,
        audits: Vec<(bool, Option<String>)>,
    }

    /// Run guarded_egress with a flag-tracking op + a capturing audit sink.
    fn run_guarded(guard: &EgressGuard, authority: EgressAuthority) -> GuardedOutcome {
        use std::cell::Cell;
        use std::sync::Mutex as StdMutex;

        let ran = Cell::new(false);
        let captured: StdMutex<Vec<(bool, Option<String>)>> = StdMutex::new(Vec::new());
        let audit = |a: EgressAudit<'_>| {
            captured.lock().unwrap().push((a.allowed, a.reason.clone()));
        };

        // Drive the async fn to completion on a current-thread runtime.
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let res = rt.block_on(guarded_egress(guard, authority, "test_op", &audit, || {
            ran.set(true);
            async { 42u32 }
        }));

        GuardedOutcome {
            res,
            ran: ran.get(),
            audits: captured.into_inner().unwrap(),
        }
    }

    #[test]
    fn guarded_armed_untainted_agent_runs_op_and_audits_allowed() {
        let g = EgressGuard::with_clock(fixed_1000);
        g.arm(30);
        let o = run_guarded(&g, EgressAuthority::Agent);
        assert_eq!(o.res, Ok(42));
        assert!(o.ran, "op must run when authorized");
        assert_eq!(o.audits, vec![(true, None)]);
    }

    #[test]
    fn guarded_unarmed_agent_denies_and_op_never_runs() {
        let g = EgressGuard::with_clock(fixed_1000);
        let o = run_guarded(&g, EgressAuthority::Agent);
        assert_eq!(o.res, Err(EgressDenied::NotArmed));
        assert!(!o.ran, "op must NOT run when denied");
        assert_eq!(o.audits.len(), 1);
        assert!(!o.audits[0].0, "audit must record allowed=false");
        assert_eq!(o.audits[0].1.as_deref(), Some("send authority is not armed"));
    }

    #[test]
    fn guarded_expired_agent_denies_and_op_never_runs() {
        let g = EgressGuard::with_clock(fixed_1000);
        g.arm(0); // deadline == now → expired
        let o = run_guarded(&g, EgressAuthority::Agent);
        assert_eq!(o.res, Err(EgressDenied::Expired(0)));
        assert!(!o.ran);
    }

    #[test]
    fn guarded_tainted_agent_denies_and_op_never_runs() {
        let g = EgressGuard::with_clock(fixed_1000);
        g.arm(30);
        g.taint(TaintReason::MessageRead);
        let o = run_guarded(&g, EgressAuthority::Agent);
        assert_eq!(o.res, Err(EgressDenied::Tainted));
        assert!(!o.ran);
    }

    #[test]
    fn guarded_poisoned_agent_denies_and_op_never_runs() {
        use std::panic::{catch_unwind, AssertUnwindSafe};
        use std::sync::Arc;

        let g = Arc::new(EgressGuard::with_clock(fixed_1000));
        g.arm(30);
        let g2 = Arc::clone(&g);
        let _ = catch_unwind(AssertUnwindSafe(|| {
            let _held = g2.inner.lock().unwrap();
            panic!("poison");
        }));

        let o = run_guarded(&g, EgressAuthority::Agent);
        assert_eq!(o.res, Err(EgressDenied::Tainted));
        assert!(!o.ran, "op must NOT run on a poisoned guard");
    }

    #[test]
    fn guarded_operator_runs_op_even_when_tainted() {
        let g = EgressGuard::with_clock(fixed_1000);
        g.taint(TaintReason::MessageRead); // tainted + disarmed — Operator must still run
        let o = run_guarded(&g, EgressAuthority::Operator);
        assert_eq!(o.res, Ok(42));
        assert!(o.ran, "Operator op must run regardless of state");
        assert_eq!(o.audits, vec![(true, None)]);
    }

    // --- quarantine_and_rearm ---------------------------------------------

    #[test]
    fn quarantine_and_rearm_sets_clean_taint_and_fresh_deadline_atomically() {
        let g = EgressGuard::with_clock(|| 1_000);
        g.arm(300); g.taint(TaintReason::MessageRead);
        assert!(g.authorize(EgressAuthority::Agent).is_err());
        let deadline = g.quarantine_and_rearm(60);
        assert_eq!(deadline, 1_060);
        assert!(!g.is_tainted());
        assert!(g.authorize(EgressAuthority::Agent).is_ok());
    }
    #[test]
    fn quarantine_and_rearm_replaces_not_extends_old_deadline() {
        let g = EgressGuard::with_clock(|| 1_000);
        g.arm(10_000); g.taint(TaintReason::MessageRead);
        assert_eq!(g.quarantine_and_rearm(60), 1_060); // not 11_000
    }
    #[test]
    fn plain_arm_still_does_not_clear_taint() {
        let g = EgressGuard::with_clock(|| 1_000);
        g.taint(TaintReason::MessageRead); g.arm(300);
        assert!(g.is_tainted());
        assert!(g.authorize(EgressAuthority::Agent).is_err());
    }

    // --- taint_reason (tuxlink-pf6re) -------------------------------------

    #[test]
    fn taint_records_the_reason() {
        let g = EgressGuard::with_clock(|| 1_000);
        assert_eq!(g.taint_reason(), None, "un-tainted guard has no reason");
        g.taint(TaintReason::SearchResults);
        assert!(g.is_tainted());
        assert_eq!(g.taint_reason(), Some(TaintReason::SearchResults));
    }

    #[test]
    fn taint_reason_is_first_wins_monotonic() {
        // A later read must NOT overwrite the original cause within a run.
        let g = EgressGuard::with_clock(|| 1_000);
        g.taint(TaintReason::MailboxList);
        g.taint(TaintReason::MessageRead);
        assert_eq!(
            g.taint_reason(),
            Some(TaintReason::MailboxList),
            "first taint wins; a later read must not clobber the recorded cause"
        );
    }

    #[test]
    fn clear_taint_resets_the_reason() {
        let g = EgressGuard::with_clock(|| 1_000);
        g.taint(TaintReason::SessionLog);
        g.clear_taint();
        assert!(!g.is_tainted());
        assert_eq!(g.taint_reason(), None);
    }

    #[test]
    fn quarantine_and_rearm_resets_the_reason() {
        let g = EgressGuard::with_clock(|| 1_000);
        g.taint(TaintReason::MessageRead);
        g.quarantine_and_rearm(60);
        assert_eq!(g.taint_reason(), None, "quarantine clears the reason with the taint");
    }

    #[test]
    fn plain_arm_preserves_the_reason() {
        // Pairs with `plain_arm_still_does_not_clear_taint`: arm touches neither
        // the taint flag nor its recorded cause.
        let g = EgressGuard::with_clock(|| 1_000);
        g.taint(TaintReason::MailboxList);
        g.arm(300);
        assert_eq!(g.taint_reason(), Some(TaintReason::MailboxList));
    }

    #[test]
    fn taint_reason_tokens_are_stable() {
        assert_eq!(TaintReason::MailboxList.as_str(), "mailbox_list");
        assert_eq!(TaintReason::MessageRead.as_str(), "message_read");
        assert_eq!(TaintReason::SearchResults.as_str(), "search_results");
        assert_eq!(TaintReason::SessionLog.as_str(), "session_log");
        assert_eq!(TaintReason::RoutinesJournal.as_str(), "routines_journal");
    }
}
