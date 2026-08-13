//! Per-datum input provenance: authorize an action against the taint of its
//! INPUTS rather than a single session-wide flag.
//!
//! This REFINES the mailbox-read-locks-send doctrine, it does not replace it.
//! Transmission is untouched. What changes is that a LOCAL write whose every
//! parameter is structurally low-bandwidth can proceed in a tainted session,
//! because the attacker chose nothing about it.
//!
//! # Why classify by type instead of tracing origin
//!
//! Real provenance cannot be recovered from the wire. The model hands us JSON
//! and we have no way to know whether it derived a value from the operator's
//! sentence, from a tool result, or from a line inside a hostile message. Any
//! scheme that asks the model to self-report provenance is asking the
//! potentially-influenced party to grade itself.
//!
//! So this classifies each parameter by the BANDWIDTH ITS TYPE PERMITS, which
//! is a property of the schema and is checkable without trusting anyone. A
//! closed enum lets an attacker steer a choice among N options. A system-issued
//! opaque handle lets them pick which of N messages. Free text lets them say
//! anything. The first two are bounded channels the ADR already accepts; the
//! third is the one worth blocking.
//!
//! # What this deliberately does NOT do
//!
//! It does not relax EGRESS. A tainted session stays blocked from transmitting
//! even when every parameter is clean, because the DECISION to transmit is
//! itself control-flow influenced by whatever the session read. That is the
//! implicit-taint residual the design record names (efk3k addendum 4), and
//! narrowing the transmit gate on parameter cleanliness alone would walk
//! straight into it.
//!
//! Nor does it make anything a classifier decides. Bandwidth classes come from
//! the tool's own schema at the call site. A model cannot argue its way into a
//! lower class.

use crate::{EgressAuthority, EgressDenied};

/// How much an attacker who controls untrusted content can steer one parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bandwidth {
    /// The human typed it this turn. Not attacker-influenced at all.
    OperatorSupplied,
    /// An opaque handle THIS SYSTEM minted, such as the conversion schema's
    /// `message_ref`. The attacker's whole influence is choosing which of the
    /// N messages already in the mailbox is referenced.
    SystemIssued,
    /// One of a fixed, code-owned set (a folder name, a transport enum). A
    /// bounded 1-of-N choice.
    ClosedEnum,
    /// Passed a strict grammar before crossing (callsign, Maidenhead grid).
    /// Bounded, and the bound is stated where the grammar is defined rather
    /// than implied to be zero.
    GrammarBound,
    /// Arbitrary attacker-supplyable content. Unbounded.
    FreeText,
}

impl Bandwidth {
    /// Whether a tainted session may act on a parameter of this class.
    ///
    /// Everything except [`Bandwidth::FreeText`] is a bounded channel the
    /// architecture already accepts elsewhere (the schema crosses closed enums
    /// and grammar-bound tokens by design). Free text is the unbounded case.
    pub fn is_bounded(self) -> bool {
        !matches!(self, Bandwidth::FreeText)
    }
}

/// One named parameter of an action, with the bandwidth its TYPE permits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Input<'a> {
    pub name: &'a str,
    pub bandwidth: Bandwidth,
}

impl<'a> Input<'a> {
    pub fn new(name: &'a str, bandwidth: Bandwidth) -> Self {
        Self { name, bandwidth }
    }
    pub fn operator(name: &'a str) -> Self {
        Self::new(name, Bandwidth::OperatorSupplied)
    }
    pub fn system(name: &'a str) -> Self {
        Self::new(name, Bandwidth::SystemIssued)
    }
    pub fn closed_enum(name: &'a str) -> Self {
        Self::new(name, Bandwidth::ClosedEnum)
    }
    pub fn grammar(name: &'a str) -> Self {
        Self::new(name, Bandwidth::GrammarBound)
    }
    pub fn free_text(name: &'a str) -> Self {
        Self::new(name, Bandwidth::FreeText)
    }
}

/// What kind of authority an action needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionClass {
    /// Anything that leaves the box: RF emit, internet send, outbox flush.
    /// Per-datum reasoning does NOT apply; the session rules are unchanged.
    Egress,
    /// A purely local state mutation: move a message between local folders,
    /// write a file into the agent's sandboxed directory. Nothing leaves.
    LocalWrite,
}

/// The taint half of the guard state, passed in so this stays a pure function
/// and can be tested without constructing a guard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionState {
    pub tainted: bool,
    /// Whether the operator's grant is currently live. Per-datum reasoning
    /// never substitutes for the grant: an unarmed session is refused exactly
    /// as before.
    pub armed: bool,
}

/// Refused because a tainted session tried to act on unbounded input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnboundedInput {
    pub name: String,
}

/// THE PER-DATUM GATE.
///
/// Rules, in order, all deterministic:
///
/// 1. The operator is always allowed. Unchanged.
/// 2. Not armed is refused. Per-datum reasoning is not a substitute for the
///    operator's grant, only a refinement of what taint costs.
/// 3. [`ActionClass::Egress`] follows the existing session rules exactly. A
///    tainted session cannot transmit, however clean its parameters look.
/// 4. [`ActionClass::LocalWrite`] in a TAINTED session is allowed if and only
///    if every parameter is bounded. One free-text parameter refuses the whole
///    action, and the refusal names the parameter.
///
/// The asymmetry between 3 and 4 is the point. Transmission hands the attacker
/// a channel out of the box, so it stays maximally conservative. A local write
/// with bounded parameters hands them a choice among options we already
/// enumerated.
pub fn authorize_action(
    authority: EgressAuthority,
    class: ActionClass,
    state: SessionState,
    inputs: &[Input<'_>],
) -> Result<(), Result<EgressDenied, UnboundedInput>> {
    if authority == EgressAuthority::Operator {
        return Ok(());
    }
    if !state.armed {
        return Err(Ok(EgressDenied::NotArmed));
    }
    match class {
        ActionClass::Egress => {
            if state.tainted {
                Err(Ok(EgressDenied::Tainted))
            } else {
                Ok(())
            }
        }
        ActionClass::LocalWrite => {
            if !state.tainted {
                return Ok(());
            }
            match inputs.iter().find(|i| !i.bandwidth.is_bounded()) {
                Some(bad) => Err(Err(UnboundedInput {
                    name: bad.name.to_string(),
                })),
                None => Ok(()),
            }
        }
    }
}

/// [`crate::guarded_egress`]'s per-datum sibling for LOCAL writes.
///
/// Same audit shape, same fail-closed posture on a poisoned lock, but the
/// taint decision consults the action's parameters. Use this ONLY for
/// operations that cannot leave the box; anything that transmits keeps
/// `guarded_egress`.
pub async fn guarded_local_write<T, F, Fut>(
    guard: &crate::EgressGuard,
    authority: EgressAuthority,
    op_label: &str,
    inputs: &[Input<'_>],
    audit: &(dyn Fn(crate::EgressAudit<'_>) + Send + Sync),
    op: F,
) -> Result<T, EgressDenied>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = T>,
{
    // One lock, both flags, fail-closed on poison. Read separately because
    // `decide` checks taint BEFORE arming, so an unarmed tainted session
    // reports Tainted and a naive "relax the Tainted case" layer would have let
    // it write with no operator grant at all.
    let state = guard.provenance_state();
    let decision = authorize_action(authority, ActionClass::LocalWrite, state, inputs);
    match decision {
        Ok(()) => {
            audit(crate::EgressAudit {
                op: op_label,
                authority,
                allowed: true,
                reason: None,
            });
            Ok(op().await)
        }
        Err(Ok(denied)) => {
            audit(crate::EgressAudit {
                op: op_label,
                authority,
                allowed: false,
                reason: Some(denied.to_string()),
            });
            Err(denied)
        }
        Err(Err(unbounded)) => {
            // Reported as Tainted so callers keep one denial type, but the
            // audit line names the offending parameter so an operator reading
            // the session log can see WHY.
            audit(crate::EgressAudit {
                op: op_label,
                authority,
                allowed: false,
                reason: Some(format!(
                    "session tainted and parameter `{}` is unbounded free text",
                    unbounded.name
                )),
            });
            Err(EgressDenied::Tainted)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ARMED_CLEAN: SessionState = SessionState {
        tainted: false,
        armed: true,
    };
    const ARMED_TAINTED: SessionState = SessionState {
        tainted: true,
        armed: true,
    };
    const DISARMED_TAINTED: SessionState = SessionState {
        tainted: true,
        armed: false,
    };

    /// The exact shape of the v26 deadlock: the operator armed, the agent read
    /// a message (tainting the session), and then every local verb was refused.
    /// 35 of 405 units, all of them armed and tainted, half the denials on
    /// verbs that transmit nothing (tuxlink-0rc3h).
    #[test]
    fn the_v26_deadlock_case_now_proceeds() {
        // mailbox_move(message_ref = system-issued handle, folder = closed enum)
        let move_inputs = [Input::system("message_ref"), Input::closed_enum("folder")];
        assert!(
            authorize_action(
                EgressAuthority::Agent,
                ActionClass::LocalWrite,
                ARMED_TAINTED,
                &move_inputs
            )
            .is_ok(),
            "a local move with only bounded parameters must survive taint"
        );
    }

    #[test]
    fn a_free_text_parameter_still_refuses_and_names_itself() {
        // The dangerous shape: the destination came from somewhere arbitrary.
        let inputs = [Input::system("message_ref"), Input::free_text("dest")];
        match authorize_action(
            EgressAuthority::Agent,
            ActionClass::LocalWrite,
            ARMED_TAINTED,
            &inputs,
        ) {
            Err(Err(u)) => assert_eq!(u.name, "dest"),
            other => panic!("expected an unbounded-input refusal, got {other:?}"),
        }
    }

    /// The property that must NOT regress. Clean parameters do not buy a
    /// transmission, because the decision to transmit is itself influenced by
    /// whatever the session read.
    #[test]
    fn taint_still_blocks_egress_even_with_perfectly_clean_inputs() {
        let pristine = [Input::operator("to"), Input::grammar("callsign")];
        assert_eq!(
            authorize_action(
                EgressAuthority::Agent,
                ActionClass::Egress,
                ARMED_TAINTED,
                &pristine
            ),
            Err(Ok(EgressDenied::Tainted)),
            "per-datum reasoning must never unlock transmit"
        );
        // Not even with no parameters at all.
        assert_eq!(
            authorize_action(
                EgressAuthority::Agent,
                ActionClass::Egress,
                ARMED_TAINTED,
                &[]
            ),
            Err(Ok(EgressDenied::Tainted))
        );
    }

    #[test]
    fn per_datum_reasoning_is_not_a_substitute_for_the_operators_grant() {
        // Bounded inputs, untainted, but never armed: still refused.
        for class in [ActionClass::Egress, ActionClass::LocalWrite] {
            assert_eq!(
                authorize_action(
                    EgressAuthority::Agent,
                    class,
                    DISARMED_TAINTED,
                    &[Input::closed_enum("folder")]
                ),
                Err(Ok(EgressDenied::NotArmed)),
                "{class:?} must still require the grant"
            );
        }
    }

    /// THE HOLE I NEARLY SHIPPED. `decide` checks taint BEFORE arming, so a
    /// session that is tainted AND never armed reports `Tainted`, not
    /// `NotArmed`. A per-datum layer built as "relax the Tainted verdict" would
    /// therefore have granted local writes to a session with no operator grant
    /// at all. `provenance_state` reads the two flags separately for exactly
    /// this reason; this test fails if anyone re-derives armed from the denial.
    #[test]
    fn a_tainted_unarmed_session_is_refused_even_with_perfectly_bounded_inputs() {
        let guard = crate::EgressGuard::with_clock(|| 1_000);
        guard.taint(crate::TaintReason::MessageRead);
        // Never armed. The session-level gate reports Tainted, masking that.
        assert_eq!(
            guard.authorize(EgressAuthority::Agent),
            Err(EgressDenied::Tainted),
            "precondition: taint masks the missing grant at the session level"
        );

        let state = guard.provenance_state();
        assert!(state.tainted);
        assert!(!state.armed, "provenance_state must see through the mask");

        assert_eq!(
            authorize_action(
                EgressAuthority::Agent,
                ActionClass::LocalWrite,
                state,
                &[Input::system("message_ref"), Input::closed_enum("folder")]
            ),
            Err(Ok(EgressDenied::NotArmed)),
            "bounded inputs must NOT substitute for the operator's grant"
        );
    }

    #[test]
    fn provenance_state_sees_an_armed_untainted_session() {
        let guard = crate::EgressGuard::with_clock(|| 1_000);
        guard.arm(300);
        let s = guard.provenance_state();
        assert!(s.armed && !s.tainted);
        // And an expired grant reads as unarmed.
        let expired = crate::EgressGuard::with_clock(|| 10_000);
        expired.arm(0);
        assert!(!expired.provenance_state().armed);
    }

    #[test]
    fn the_operator_is_never_gated_by_any_of_this() {
        for class in [ActionClass::Egress, ActionClass::LocalWrite] {
            assert!(authorize_action(
                EgressAuthority::Operator,
                class,
                DISARMED_TAINTED,
                &[Input::free_text("anything")]
            )
            .is_ok());
        }
    }

    #[test]
    fn an_untainted_session_is_unaffected_by_bandwidth() {
        // Free text is only interesting once the session is tainted. Before
        // that it is just a parameter.
        assert!(authorize_action(
            EgressAuthority::Agent,
            ActionClass::LocalWrite,
            ARMED_CLEAN,
            &[Input::free_text("subject")]
        )
        .is_ok());
    }

    #[test]
    fn every_bounded_class_survives_taint_and_free_text_does_not() {
        for b in [
            Bandwidth::OperatorSupplied,
            Bandwidth::SystemIssued,
            Bandwidth::ClosedEnum,
            Bandwidth::GrammarBound,
        ] {
            assert!(b.is_bounded(), "{b:?} should be bounded");
            assert!(authorize_action(
                EgressAuthority::Agent,
                ActionClass::LocalWrite,
                ARMED_TAINTED,
                &[Input::new("p", b)]
            )
            .is_ok());
        }
        assert!(!Bandwidth::FreeText.is_bounded());
    }

    /// One free-text parameter poisons the whole call, even buried among
    /// bounded ones, and the FIRST unbounded one is what gets named.
    #[test]
    fn a_single_unbounded_parameter_refuses_the_whole_action() {
        let inputs = [
            Input::system("a"),
            Input::closed_enum("b"),
            Input::free_text("c"),
            Input::free_text("d"),
        ];
        match authorize_action(
            EgressAuthority::Agent,
            ActionClass::LocalWrite,
            ARMED_TAINTED,
            &inputs,
        ) {
            Err(Err(u)) => assert_eq!(u.name, "c"),
            other => panic!("expected refusal naming `c`, got {other:?}"),
        }
    }
}
